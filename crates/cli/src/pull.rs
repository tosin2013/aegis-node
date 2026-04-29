//! `aegis pull` — fetch + verify a model artifact from an OCI registry.
//!
//! Per ADR-013 (OCI Artifacts for Model Distribution) and OCI-A
//! ([issue #66](https://github.com/tosin2013/aegis-node/issues/66)).
//!
//! Phase 1 ships a shell-out implementation around the `oras` and
//! `cosign` binaries that the supply-chain workflow already requires
//! (per ADR-017). An embedded OCI client (no shell-out) is a
//! follow-up.
//!
//! ## Verification flow
//!
//! 1. **Refuse refs without a `@sha256:` digest pin.** Tags can move;
//!    pulling a moving target invalidates the F1 digest binding.
//! 2. **Stage the pull in a temp dir.** No partial state ever touches
//!    the cache.
//! 3. **Run `oras pull`** to fetch the descriptor + blob.
//! 4. **Run `cosign verify`** against the configured key (or keyless
//!    via Sigstore's public Fulcio + Rekor — keyless is the default
//!    when `--cosign-key` is omitted, matching ADR-017).
//! 5. **Recompute SHA-256 of the blob** and compare to the ref's
//!    pinned digest. A successful `cosign verify` on the descriptor
//!    is necessary; matching the pinned digest is what closes the
//!    end-to-end chain (the pinned digest is what F1 binds into the
//!    SVID extension at boot).
//! 6. **Move into the content-addressed cache** at
//!    `<cache-dir>/<sha256-hex>/blob.bin`. The atomic rename keeps
//!    the cache consistent — either the artifact is fully verified
//!    and present, or it isn't.
//!
//! Each step has an explicit error variant so the F1 boot path can
//! refuse with a clear violation reason if the artifact a session
//! references isn't actually present + verified.

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};
use thiserror::Error;

/// Outcome of a successful `aegis pull`. The cache path is what
/// callers (and `Session::boot`) consume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PulledArtifact {
    /// The reference the operator passed in.
    pub reference: ParsedRef,
    /// Where the verified blob lives on disk.
    pub blob_path: PathBuf,
    /// SHA-256 digest of the blob, lowercase hex.
    pub sha256_hex: String,
}

/// Typed errors. Surface mapped 1:1 to verification gates so an
/// operator (or the F1 boot path) sees exactly which step refused.
#[derive(Debug, Error)]
pub enum PullError {
    /// The required external tool (oras / cosign) isn't on $PATH.
    #[error("required tool not found on $PATH: {tool} (install per docs/SUPPLY_CHAIN.md)")]
    MissingTool { tool: &'static str },

    /// Reference cannot be parsed at all.
    #[error("invalid reference {reference:?}: {reason}")]
    BadRef {
        reference: String,
        reason: &'static str,
    },

    /// Reference has no `@sha256:` digest. Refused to enforce the F1
    /// binding (a moving tag would invalidate the SVID extension).
    #[error("reference {reference:?} is missing an @sha256: pin (refusing — tags can move)")]
    UnpinnedRef { reference: String },

    /// `oras pull` exited non-zero. stderr is captured so an operator
    /// can debug registry / network / auth issues directly.
    #[error("oras pull failed (exit {exit_code:?}): {stderr}")]
    OrasFailed {
        exit_code: Option<i32>,
        stderr: String,
    },

    /// `cosign verify` exited non-zero — signature missing, key
    /// mismatch, or Rekor entry absent. The boot path MUST refuse on
    /// this; a session that loads an unsigned model fails the F1
    /// promise.
    #[error("cosign verify failed (exit {exit_code:?}): {stderr}")]
    CosignVerifyFailed {
        exit_code: Option<i32>,
        stderr: String,
    },

    /// The pulled blob's SHA-256 doesn't match the digest pinned in
    /// the reference. Either the registry is misbehaving or the OCI
    /// descriptor was swapped post-signing — either way, refuse.
    #[error("sha256 mismatch: ref pinned {expected}, computed {got} (artifact discarded)")]
    Sha256Mismatch { expected: String, got: String },

    /// Catch-all for filesystem errors (creating temp dirs, moves,
    /// etc.). Wrapped so callers don't have to match `io::Error` raw.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, PullError>;

/// Parsed view of an OCI reference.
///
/// Format accepted (Phase 1):
///
/// ```text
/// <registry-host>[:port]/<repo>[:<tag>]@sha256:<64 hex chars>
/// ```
///
/// `@sha256:` is mandatory. A bare-tag form (`...:v1` without `@sha256`)
/// is rejected with [`PullError::UnpinnedRef`] — the boot path needs a
/// stable digest to bind into the SVID, and a tag can move out from
/// under us.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRef {
    pub registry: String,
    pub repository: String,
    pub tag: Option<String>,
    pub sha256_hex: String,
}

impl ParsedRef {
    pub fn parse(s: &str) -> Result<Self> {
        // Split off the @sha256: digest first — required.
        let (head, digest) = match s.split_once('@') {
            Some((h, d)) => (h, d),
            None => {
                return Err(PullError::UnpinnedRef {
                    reference: s.to_string(),
                })
            }
        };
        let hex = digest
            .strip_prefix("sha256:")
            .ok_or_else(|| PullError::BadRef {
                reference: s.to_string(),
                reason: "digest must use the sha256: prefix",
            })?;
        if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(PullError::BadRef {
                reference: s.to_string(),
                reason: "sha256 must be 64 lowercase hex chars",
            });
        }

        // head is `registry[:port]/repo[:tag]`. Find the registry
        // boundary — first `/` that doesn't lie inside a `:port` of
        // the registry (registries always have at least one '.', so
        // we use the first '/' after the host portion).
        let slash = head.find('/').ok_or_else(|| PullError::BadRef {
            reference: s.to_string(),
            reason: "missing /<repo> after the registry",
        })?;
        let registry = &head[..slash];
        let repo_and_tag = &head[slash + 1..];
        if registry.is_empty() {
            return Err(PullError::BadRef {
                reference: s.to_string(),
                reason: "registry component is empty",
            });
        }
        if repo_and_tag.is_empty() {
            return Err(PullError::BadRef {
                reference: s.to_string(),
                reason: "repository component is empty",
            });
        }
        // Optional :tag inside the path part — find the LAST `:` so a
        // path containing `:` (rare) isn't broken. In practice only
        // tags carry a colon here.
        let (repository, tag) = match repo_and_tag.rsplit_once(':') {
            Some((r, t)) => (r, Some(t.to_string())),
            None => (repo_and_tag, None),
        };
        if repository.is_empty() {
            return Err(PullError::BadRef {
                reference: s.to_string(),
                reason: "repository component is empty",
            });
        }

        Ok(Self {
            registry: registry.to_string(),
            repository: repository.to_string(),
            tag,
            sha256_hex: hex.to_lowercase(),
        })
    }

    /// Reconstruct the canonical reference string for passing to oras.
    pub fn canonical(&self) -> String {
        let tag_part = self
            .tag
            .as_ref()
            .map(|t| format!(":{t}"))
            .unwrap_or_default();
        format!(
            "{}/{}{}@sha256:{}",
            self.registry, self.repository, tag_part, self.sha256_hex
        )
    }
}

/// Configuration for a single pull. Mirrors the CLI flags so the
/// library API and `aegis pull` stay in lockstep.
#[derive(Debug, Clone)]
pub struct PullConfig {
    /// Where verified blobs live. Default
    /// `$XDG_CACHE_HOME/aegis/models` (or platform equivalent).
    pub cache_dir: PathBuf,
    /// Path to the cosign public key. None ⇒ keyless verification via
    /// Sigstore's public Fulcio + Rekor (recommended for community
    /// artifacts; an org pinning their own key passes Some(path)).
    pub cosign_key: Option<PathBuf>,
    /// Identity (subject email/SPIFFE) expected on the keyless cert.
    /// Required when `cosign_key` is None — keyless verification
    /// without an identity check accepts any Fulcio cert. None +
    /// keyless means cosign's `--certificate-identity-regexp=.*`,
    /// which the operator should override in production.
    pub keyless_identity: Option<String>,
    /// OIDC issuer expected on the keyless cert. Same caveat as
    /// `keyless_identity`.
    pub keyless_oidc_issuer: Option<String>,
}

/// Pull + verify + cache. Returns the path of the verified blob.
///
/// Side-effects: spawns `oras` and `cosign` as subprocesses, writes
/// to `cache_dir`. Atomic from the cache's perspective — the move
/// into `<cache-dir>/<sha256>/blob.bin` happens only after every
/// verification step succeeds.
pub fn pull(reference: &str, cfg: &PullConfig) -> Result<PulledArtifact> {
    require_tool("oras")?;
    require_tool("cosign")?;
    let parsed = ParsedRef::parse(reference)?;

    let target_dir = cfg.cache_dir.join(&parsed.sha256_hex);
    let target_blob = target_dir.join("blob.bin");
    if target_blob.exists() {
        // Already cached + verified at some point. We re-verify the
        // sha256 so a corrupted local cache fails fast rather than
        // booting against a tampered file.
        let got = sha256_file(&target_blob)?;
        if got != parsed.sha256_hex {
            return Err(PullError::Sha256Mismatch {
                expected: parsed.sha256_hex.clone(),
                got,
            });
        }
        return Ok(PulledArtifact {
            reference: parsed.clone(),
            blob_path: target_blob,
            sha256_hex: parsed.sha256_hex,
        });
    }

    // Stage in a sibling temp dir so a partial pull never pollutes
    // the cache root.
    std::fs::create_dir_all(&cfg.cache_dir)?;
    let staging = tempfile::tempdir_in(&cfg.cache_dir)?;
    run_oras_pull(&parsed, staging.path())?;
    run_cosign_verify(&parsed, cfg)?;

    // The blob's filename is decided by the OCI manifest. We pick the
    // largest file in the staging dir — for our use case (single-blob
    // model artifacts) that's the model. Multi-blob artifacts are out
    // of scope for OCI-A.
    let blob = pick_largest_file(staging.path())?;
    let computed = sha256_file(&blob)?;
    if computed != parsed.sha256_hex {
        return Err(PullError::Sha256Mismatch {
            expected: parsed.sha256_hex.clone(),
            got: computed,
        });
    }

    std::fs::create_dir_all(&target_dir)?;
    // Persist a small metadata file alongside the blob for traceability.
    std::fs::write(target_dir.join("ref.txt"), parsed.canonical().as_bytes())?;
    std::fs::rename(&blob, &target_blob)?;

    Ok(PulledArtifact {
        reference: parsed.clone(),
        blob_path: target_blob,
        sha256_hex: parsed.sha256_hex,
    })
}

fn require_tool(tool: &'static str) -> Result<()> {
    if which(tool).is_some() {
        Ok(())
    } else {
        Err(PullError::MissingTool { tool })
    }
}

fn which(tool: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(tool);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn run_oras_pull(parsed: &ParsedRef, into: &Path) -> Result<()> {
    let out = Command::new("oras")
        .arg("pull")
        .arg("-o")
        .arg(into)
        .arg(parsed.canonical())
        .output()?;
    if !out.status.success() {
        return Err(PullError::OrasFailed {
            exit_code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        });
    }
    Ok(())
}

fn run_cosign_verify(parsed: &ParsedRef, cfg: &PullConfig) -> Result<()> {
    let mut cmd = Command::new("cosign");
    cmd.arg("verify");
    if let Some(key) = &cfg.cosign_key {
        cmd.arg("--key").arg(key);
    } else {
        // Keyless verification path. Operators in production should
        // pin an identity + issuer; community / first-attempt usage
        // can omit them but cosign will print a warning.
        if let Some(id) = &cfg.keyless_identity {
            cmd.arg("--certificate-identity-regexp").arg(id);
        } else {
            cmd.arg("--certificate-identity-regexp").arg(".*");
        }
        if let Some(issuer) = &cfg.keyless_oidc_issuer {
            cmd.arg("--certificate-oidc-issuer-regexp").arg(issuer);
        } else {
            cmd.arg("--certificate-oidc-issuer-regexp").arg(".*");
        }
    }
    cmd.arg(parsed.canonical());
    let out = cmd.output()?;
    if !out.status.success() {
        return Err(PullError::CosignVerifyFailed {
            exit_code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        });
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut f = std::fs::File::open(path)?;
    std::io::copy(&mut f, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

fn pick_largest_file(dir: &Path) -> Result<PathBuf> {
    let mut best: Option<(u64, PathBuf)> = None;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if !meta.is_file() {
            continue;
        }
        let size = meta.len();
        if best.as_ref().map_or(true, |(s, _)| size > *s) {
            best = Some((size, entry.path()));
        }
    }
    best.map(|(_, p)| p).ok_or_else(|| {
        PullError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "oras pull produced no files in staging dir",
        ))
    })
}

/// Default cache directory: `$XDG_CACHE_HOME/aegis/models` (linux),
/// `~/Library/Caches/aegis/models` (macOS), or
/// `%LOCALAPPDATA%/aegis/models` (Windows).
pub fn default_cache_dir() -> Result<PathBuf> {
    let base = dirs::cache_dir().ok_or_else(|| {
        PullError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "could not resolve user cache dir",
        ))
    })?;
    Ok(base.join("aegis").join("models"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn parsed_ref_round_trips_canonical() {
        let raw = "ghcr.io/example/qwen2.5-1.5b-q4_k_m@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let parsed = ParsedRef::parse(raw).unwrap();
        assert_eq!(parsed.registry, "ghcr.io");
        assert_eq!(parsed.repository, "example/qwen2.5-1.5b-q4_k_m");
        assert!(parsed.tag.is_none());
        assert_eq!(parsed.canonical(), raw);
    }

    #[test]
    fn parsed_ref_with_tag_and_port() {
        let raw = "registry.local:5000/team/model:v1@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let parsed = ParsedRef::parse(raw).unwrap();
        assert_eq!(parsed.registry, "registry.local:5000");
        assert_eq!(parsed.repository, "team/model");
        assert_eq!(parsed.tag.as_deref(), Some("v1"));
        assert_eq!(parsed.canonical(), raw);
    }

    #[test]
    fn parsed_ref_uppercase_hex_normalized() {
        let raw =
            "ghcr.io/x/m@sha256:ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let parsed = ParsedRef::parse(raw).unwrap();
        assert_eq!(
            parsed.sha256_hex,
            "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
        );
    }

    #[test]
    fn parsed_ref_rejects_missing_digest() {
        let err = ParsedRef::parse("ghcr.io/example/m:v1").unwrap_err();
        assert!(matches!(err, PullError::UnpinnedRef { .. }), "{err}");
    }

    #[test]
    fn parsed_ref_rejects_short_digest() {
        let err = ParsedRef::parse("ghcr.io/x/m@sha256:abc").unwrap_err();
        assert!(matches!(err, PullError::BadRef { .. }), "{err}");
    }

    #[test]
    fn parsed_ref_rejects_non_hex_digest() {
        let err = ParsedRef::parse(
            "ghcr.io/x/m@sha256:zzzz0000000000000000000000000000000000000000000000000000000000zz",
        )
        .unwrap_err();
        assert!(matches!(err, PullError::BadRef { .. }), "{err}");
    }

    #[test]
    fn parsed_ref_rejects_non_sha256_digest_algorithm() {
        let err =
            ParsedRef::parse("ghcr.io/x/m@sha512:0123456789abcdef0123456789abcdef").unwrap_err();
        assert!(matches!(err, PullError::BadRef { .. }), "{err}");
    }

    #[test]
    fn parsed_ref_rejects_empty_registry() {
        let err = ParsedRef::parse(
            "/x/m@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .unwrap_err();
        assert!(matches!(err, PullError::BadRef { .. }), "{err}");
    }

    #[test]
    fn pull_refuses_when_oras_missing() {
        // Override PATH to an empty dir so `which("oras")` fails.
        let dir = tempfile::tempdir().unwrap();
        let original = std::env::var_os("PATH");
        std::env::set_var("PATH", dir.path());
        let cfg = PullConfig {
            cache_dir: dir.path().join("cache"),
            cosign_key: None,
            keyless_identity: None,
            keyless_oidc_issuer: None,
        };
        let res = pull(
            "ghcr.io/x/m@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            &cfg,
        );
        if let Some(p) = original {
            std::env::set_var("PATH", p);
        } else {
            std::env::remove_var("PATH");
        }
        match res {
            Err(PullError::MissingTool { tool }) => assert_eq!(tool, "oras"),
            other => panic!("expected MissingTool, got {other:?}"),
        }
    }
}
