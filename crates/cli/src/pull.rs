//! `aegis pull` — fetch + verify a model artifact from an OCI registry.
//!
//! Per ADR-013 (OCI Artifacts for Model Distribution), ADR-021 (HF →
//! OCI mirror), ADR-022 (trust-boundary format agnosticism), and
//! issues [#66](https://github.com/tosin2013/aegis-node/issues/66) /
//! [#67](https://github.com/tosin2013/aegis-node/issues/67).
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
//! 3. **Fetch the manifest** via `oras manifest fetch`, parse its JSON,
//!    extract the `dev.aegis-node.chat-template.sha256` annotation. For
//!    artifacts whose media type is `MODEL_GGUF_MEDIA_TYPE` the
//!    annotation is required (publisher misconfiguration → typed
//!    `MissingChatTemplateAnnotation` refusal); for non-model artifacts
//!    (e.g. devbox image) it is optional. Per ADR-022: the runtime
//!    trust boundary verifies a signed claim; it never parses the GGUF.
//! 4. **Run `oras pull`** to fetch the descriptor + blob. `oras`
//!    verifies each pulled blob against the manifest's layer
//!    descriptor — that's where blob-bytes-vs-manifest integrity
//!    happens.
//! 5. **Run `cosign verify`** against the configured key (or keyless
//!    via Sigstore's public Fulcio + Rekor — keyless is the default
//!    when `--cosign-key` is omitted, matching ADR-017). cosign
//!    verifies the manifest's signature, which transitively covers
//!    the layer descriptors *and* every annotation read in step 3.
//! 6. **Compute the blob's SHA-256** and persist it as a sidecar
//!    (`sha256.txt`) for the F1 boot path's SVID-binding use and
//!    for cache-hit re-verification on subsequent pulls. We do NOT
//!    compare this against the ref's `@sha256:` — that's the
//!    *manifest* digest (per OCI spec), a different value from the
//!    blob's content hash. cosign + oras together provide the
//!    integrity guarantee; we surface the blob's hash for callers.
//! 7. **Persist the chat-template digest** (when present) as
//!    `chat_template.sha256.txt` for the F1 boot binding (OCI-B (b)).
//! 8. **Move into the cache** at `<cache-dir>/<manifest-sha>/blob.bin`.
//!    The atomic rename keeps the cache consistent — either the
//!    artifact is fully verified and present, or it isn't.
//!
//! On cache hits, we recompute the blob's SHA-256 and compare to the
//! sidecar — catches local-disk tampering between pulls.
//!
//! Each step has an explicit error variant so the F1 boot path can
//! refuse with a clear violation reason if the artifact a session
//! references isn't actually present + verified.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Filename of the per-pull provenance sidecar written into the
/// model cache directory by [`pull`]. Captures the OCI ref + cosign
/// verification configuration + pull timestamp so the WebUI's Model
/// Library (per [ADR-032](../../docs/adrs/032-webui-model-library-and-session-forking.md))
/// can surface "verified by …" for each cached model without
/// re-running cosign or re-parsing the registry manifest. Cache
/// entries from before this sidecar was introduced fall back to
/// the older `ref.txt` for `oci_ref` only — no cosign metadata.
pub const PROVENANCE_FILENAME: &str = "provenance.json";

/// Schema version for [`Provenance`]. Bumped when the on-disk format
/// changes incompatibly. Readers (the ui-server's Models handler)
/// MUST reject unknown major versions rather than silently misread.
pub const PROVENANCE_SCHEMA_VERSION: u32 = 1;

/// On-disk record written next to each cached model's `blob.bin`.
/// Persists the verification context [`pull`] established so that
/// downstream consumers — the WebUI Model Library, future evidence-
/// pack generators — don't need to re-run cosign or contact the
/// registry to know how the blob got here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Provenance {
    /// Format version. Currently always [`PROVENANCE_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Canonical OCI reference the operator pulled (with `@sha256:` pin).
    pub oci_ref: String,
    /// Manifest digest extracted from the reference. Same value as
    /// the cache subdirectory name; included here so the file is
    /// self-describing without requiring path-context to read.
    pub manifest_digest: String,
    /// SHA-256 of `blob.bin`, lowercase hex. Equals
    /// [`PulledArtifact::sha256_hex`] at write time.
    pub blob_sha256: String,
    /// Optional chat-template hash extracted from the cosign-covered
    /// manifest annotation `dev.aegis-node.chat-template.sha256`.
    /// `None` for non-model artifacts.
    pub chat_template_sha256: Option<String>,
    /// How cosign verified the manifest signature.
    pub cosign: CosignVerification,
    /// RFC3339 UTC timestamp at which the pull completed. Useful for
    /// the Model Library's "last used" / "pulled at" columns and for
    /// evidence-pack generation later.
    pub pulled_at: String,
}

/// The cosign-verification configuration that succeeded at pull
/// time. `verified` is always `true` in a written provenance file —
/// [`pull`] only writes the sidecar after cosign returned exit 0.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CosignVerification {
    /// Whether cosign verification succeeded. Always `true` in a
    /// written sidecar (failed pulls never reach the persist step).
    /// Kept as a field so future consumers can pattern-match
    /// without inferring from the file's mere existence.
    pub verified: bool,
    /// Verification mode used: keyed (operator-supplied public key)
    /// or keyless (Sigstore Fulcio + Rekor).
    pub mode: CosignMode,
    /// Path to the operator-supplied public key, if the keyed mode
    /// was used. `None` for keyless.
    pub key_path: Option<String>,
    /// Identity-regex constraint passed to cosign in keyless mode
    /// (`--certificate-identity-regexp`). `None` for keyed.
    pub keyless_identity_pattern: Option<String>,
    /// OIDC-issuer-regex constraint passed to cosign in keyless mode
    /// (`--certificate-oidc-issuer-regexp`). `None` for keyed.
    pub keyless_oidc_issuer_pattern: Option<String>,
}

/// Cosign verification mode. Kept as a separate enum (rather than an
/// `Option<KeyPath>` union) so the JSON shape names the choice
/// explicitly — auditors reading the sidecar see "keyless" rather
/// than inferring it from a missing field.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CosignMode {
    /// Keyed verification — `--cosign-key <path>` was supplied.
    Key,
    /// Keyless verification — Sigstore Fulcio + Rekor (the default).
    Keyless,
}

/// OCI media type for a single-blob GGUF model artifact published by
/// Aegis-Node's `models-publish.yml` (per ADR-021). Publishers that use
/// this media type MUST also set the chat-template annotation —
/// `aegis pull` enforces that in `extract_chat_template_annotation`.
pub const MODEL_GGUF_MEDIA_TYPE: &str = "application/vnd.aegis-node.model.gguf.v1";

/// OCI media type for a single-blob `.litertlm` model artifact (the
/// LiteRT-LM family — Gemma 4 etc., per ADR-023 §"Implementation
/// plan" item 3 and LiteRT-C / [#97](https://github.com/tosin2013/aegis-node/issues/97)).
/// Same publisher-side annotation requirement as
/// [`MODEL_GGUF_MEDIA_TYPE`]: the
/// `dev.aegis-node.chat-template.sha256` annotation is mandatory and
/// `aegis pull` enforces it in `extract_chat_template_annotation`.
pub const MODEL_LITERTLM_MEDIA_TYPE: &str = "application/vnd.aegis-node.model.litertlm.v1";

/// All OCI artifact-types `aegis pull` recognizes as **model**
/// artifacts (i.e., subject to the chat-template annotation
/// requirement). Adding a new family requires (a) a new constant
/// here, (b) a corresponding `format` branch in `models-publish.yml`,
/// and (c) the F1 boot path in `aegis-inference-engine` honoring the
/// extracted SHA the same way as for GGUF.
pub const MODEL_ARTIFACT_TYPES: &[&str] = &[MODEL_GGUF_MEDIA_TYPE, MODEL_LITERTLM_MEDIA_TYPE];

/// OCI manifest annotation carrying the SHA-256 of the GGUF's
/// `tokenizer.chat_template` bytes. Defends against template-only
/// poisoning per ADR-013 §"Decision" item 4 and ADR-022 §"Decision":
/// the trust boundary verifies a signed claim instead of parsing the
/// GGUF itself.
pub const CHAT_TEMPLATE_SHA_ANNOTATION: &str = "dev.aegis-node.chat-template.sha256";

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
    /// SHA-256 of the model's chat template, lowercase hex — read
    /// from the cosign-covered manifest annotation
    /// `dev.aegis-node.chat-template.sha256`. `None` for non-model
    /// artifacts (e.g., the devbox image). Required for artifacts
    /// whose media type is in [`MODEL_ARTIFACT_TYPES`] (currently
    /// GGUF + `.litertlm`). Defends against template-only poisoning
    /// per ADR-013 §"Decision" item 4. The runtime never parses the
    /// model bytes (per ADR-022): we trust the publisher's signed
    /// claim, defended in depth by the backend's own parser at
    /// inference time. For GGUF the publisher reads
    /// `tokenizer.chat_template` from the model itself; for
    /// `.litertlm` the publisher hashes the sibling
    /// `chat_template.jinja` file in the HF repo (the same content
    /// embedded in the .litertlm flatbuffer's
    /// `LlmMetadata.jinja_prompt_template` field).
    pub chat_template_sha256_hex: Option<String>,
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

    /// `oras manifest fetch` exited non-zero, or its output isn't valid
    /// JSON. We need the manifest to read the chat-template annotation;
    /// without it the F1 binding would be unmoored.
    #[error("oras manifest fetch failed: {detail}")]
    ManifestFetchFailed { detail: String },

    /// Manifest declares the model-GGUF media type but is missing the
    /// `dev.aegis-node.chat-template.sha256` annotation. Per ADR-022
    /// the publisher is responsible for setting this; refusing here
    /// means a misconfigured publish surfaces loudly instead of leaving
    /// a session unboundable.
    #[error(
        "manifest declares model artifact-type {media_type:?} but lacks the \
         {annotation:?} annotation (publisher misconfiguration; \
         re-run models-publish.yml or analogous)"
    )]
    MissingChatTemplateAnnotation {
        media_type: String,
        annotation: &'static str,
    },

    /// The annotation is present but its value isn't a 64-char
    /// lowercase hex SHA-256. We refuse rather than guess what the
    /// publisher meant.
    #[error("manifest annotation {annotation:?} is not a 64-char hex sha256: {value:?}")]
    InvalidAnnotationValue {
        annotation: &'static str,
        value: String,
    },

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
/// Integrity model: cosign + oras already provide end-to-end integrity
/// together — cosign verifies the manifest's signature (including its
/// layer descriptors), and oras verifies each pulled blob against the
/// matching layer descriptor. We do NOT re-verify the blob's SHA-256
/// against the reference, because the reference's `@sha256:` carries
/// the **manifest digest** (per OCI spec), not the blob digest. They
/// are different values and comparing them was a category error.
///
/// We do still compute and surface the blob's SHA-256 — the F1 boot
/// path needs that digest for the SVID extension binding — and we
/// persist it alongside the blob so subsequent cache hits can detect
/// local-disk tampering.
///
/// Side-effects: spawns `oras` and `cosign` as subprocesses, writes
/// to `cache_dir`. Atomic from the cache's perspective — the move
/// into `<cache-dir>/<manifest-sha>/blob.bin` happens only after
/// every verification step succeeds.
pub fn pull(reference: &str, cfg: &PullConfig) -> Result<PulledArtifact> {
    require_tool("oras")?;
    require_tool("cosign")?;
    let parsed = ParsedRef::parse(reference)?;

    // Cache key is the manifest digest from the reference. That's
    // what callers pin and what's stable across pulls. The blob's
    // own SHA-256 is captured in a sidecar (`sha256.txt`) for
    // tamper-detection on cache hits.
    let target_dir = cfg.cache_dir.join(&parsed.sha256_hex);
    let target_blob = target_dir.join("blob.bin");
    let sidecar = target_dir.join("sha256.txt");
    if target_blob.exists() && sidecar.exists() {
        // Cache hit: re-verify the blob's SHA-256 against the sidecar
        // recorded at original-pull time. Catches local-disk tampering
        // (someone overwriting blob.bin with different bytes after
        // the original pull). The sidecar is the source of truth
        // here — it was written under the same cosign-verified pull
        // that produced the blob.
        let recorded = std::fs::read_to_string(&sidecar)?.trim().to_string();
        let got = sha256_file(&target_blob)?;
        if got != recorded {
            return Err(PullError::Sha256Mismatch {
                expected: recorded,
                got,
            });
        }
        let chat_template_sha256_hex = read_chat_template_sidecar(&target_dir)?;
        return Ok(PulledArtifact {
            reference: parsed.clone(),
            blob_path: target_blob,
            sha256_hex: recorded,
            chat_template_sha256_hex,
        });
    }

    // Fetch the cosign-covered manifest and extract the chat-template
    // annotation BEFORE pulling the blob. cosign verifies the manifest's
    // signature (which transitively covers all annotations); reading the
    // annotation here means the trust-boundary code in this function
    // never has to parse the GGUF itself (per ADR-022).
    let manifest = run_oras_manifest_fetch(&parsed)?;
    let chat_template_sha256_hex = extract_chat_template_annotation(&manifest)?;

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
    let blob_sha256_hex = sha256_file(&blob)?;

    std::fs::create_dir_all(&target_dir)?;
    // Persist sidecars alongside the blob:
    //   ref.txt                   — canonical reference, for traceability
    //   sha256.txt                — blob's actual SHA-256, used by the
    //                               F1 boot path and by future cache-hit
    //                               re-verification
    //   chat_template.sha256.txt  — chat-template SHA-256 read from the
    //                               cosign-covered manifest annotation
    //                               (Some for model artifacts; absent
    //                               for non-model artifacts like devbox)
    std::fs::write(target_dir.join("ref.txt"), parsed.canonical().as_bytes())?;
    std::fs::write(&sidecar, blob_sha256_hex.as_bytes())?;
    if let Some(hex) = &chat_template_sha256_hex {
        std::fs::write(target_dir.join("chat_template.sha256.txt"), hex.as_bytes())?;
    }
    std::fs::rename(&blob, &target_blob)?;

    // Persist provenance LAST — its presence implies the blob and
    // every other sidecar are in place. Downstream readers (the
    // ui-server's Models handler per ADR-032) treat a missing
    // provenance.json as "legacy cache entry" and fall back to
    // ref.txt; partial-write recovery scans treat its absence as
    // "incomplete pull, needs cleanup."
    let provenance = Provenance {
        schema_version: PROVENANCE_SCHEMA_VERSION,
        oci_ref: parsed.canonical(),
        manifest_digest: parsed.sha256_hex.clone(),
        blob_sha256: blob_sha256_hex.clone(),
        chat_template_sha256: chat_template_sha256_hex.clone(),
        cosign: cosign_record(cfg),
        pulled_at: rfc3339_now(),
    };
    write_provenance(&target_dir, &provenance)?;

    Ok(PulledArtifact {
        reference: parsed.clone(),
        blob_path: target_blob,
        sha256_hex: blob_sha256_hex,
        chat_template_sha256_hex,
    })
}

/// Build the [`CosignVerification`] record from the operator's
/// [`PullConfig`]. Verified is hardcoded to `true` because we only
/// reach this code path after [`run_cosign_verify`] returned exit 0.
fn cosign_record(cfg: &PullConfig) -> CosignVerification {
    if let Some(key) = &cfg.cosign_key {
        CosignVerification {
            verified: true,
            mode: CosignMode::Key,
            key_path: Some(key.display().to_string()),
            keyless_identity_pattern: None,
            keyless_oidc_issuer_pattern: None,
        }
    } else {
        CosignVerification {
            verified: true,
            mode: CosignMode::Keyless,
            key_path: None,
            // Match the regex-default fallbacks used in
            // `run_cosign_verify` so the recorded values reflect
            // exactly what cosign was told.
            keyless_identity_pattern: Some(
                cfg.keyless_identity
                    .clone()
                    .unwrap_or_else(|| ".*".to_string()),
            ),
            keyless_oidc_issuer_pattern: Some(
                cfg.keyless_oidc_issuer
                    .clone()
                    .unwrap_or_else(|| ".*".to_string()),
            ),
        }
    }
}

/// Atomically write a [`Provenance`] record into the cache dir.
/// Uses a temp-file-and-rename so a partial write never produces a
/// truncated `provenance.json`.
pub fn write_provenance(dir: &Path, prov: &Provenance) -> Result<()> {
    let target = dir.join(PROVENANCE_FILENAME);
    let json = serde_json::to_vec_pretty(prov)
        .map_err(|e| PullError::Io(std::io::Error::other(format!("serialize provenance: {e}"))))?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".provenance.")
        .suffix(".tmp")
        .tempfile_in(dir)?;
    use std::io::Write;
    tmp.write_all(&json)?;
    tmp.flush()?;
    tmp.persist(&target).map_err(|e| PullError::Io(e.error))?;
    Ok(())
}

/// Read + validate the provenance sidecar from a cache dir. Returns
/// `Ok(None)` for a legacy entry that doesn't have one, `Err` only
/// for genuine I/O / parse failures. Schema-version mismatch is an
/// error — readers shouldn't try to interpret an unknown major.
pub fn read_provenance(dir: &Path) -> Result<Option<Provenance>> {
    let path = dir.join(PROVENANCE_FILENAME);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(PullError::Io(e)),
    };
    let prov: Provenance = serde_json::from_slice(&bytes).map_err(|e| {
        PullError::Io(std::io::Error::other(format!(
            "parsing {}: {e}",
            path.display(),
        )))
    })?;
    if prov.schema_version != PROVENANCE_SCHEMA_VERSION {
        return Err(PullError::Io(std::io::Error::other(format!(
            "{} schema_version={} unsupported (expected {})",
            path.display(),
            prov.schema_version,
            PROVENANCE_SCHEMA_VERSION,
        ))));
    }
    Ok(Some(prov))
}

/// RFC3339 timestamp without pulling in `chrono`. Uses the same
/// pure-stdlib civil-from-days conversion that
/// `crates/ui-server/src/handlers/models.rs` uses; kept independent
/// here so pull.rs has no cross-crate timestamp dep.
fn rfc3339_now() -> String {
    let dur = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs() as i64;
    let nanos = dur.subsec_nanos();
    naive_rfc3339_from_unix(secs, nanos)
}

fn naive_rfc3339_from_unix(secs: i64, nanos: u32) -> String {
    const SECONDS_PER_DAY: i64 = 86_400;
    let days = secs.div_euclid(SECONDS_PER_DAY);
    let time_of_day = secs.rem_euclid(SECONDS_PER_DAY);
    let hh = time_of_day / 3600;
    let mm = (time_of_day % 3600) / 60;
    let ss = time_of_day % 60;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.{millis:03}Z",
        millis = nanos / 1_000_000,
    )
}

/// Read the chat-template sidecar from a populated cache dir, if it
/// exists. Cache hits reuse the sidecar rather than re-fetching the
/// manifest — the blob's own SHA-256 is re-verified above, and the
/// sidecar was written from a cosign-verified annotation under the
/// original pull.
fn read_chat_template_sidecar(dir: &Path) -> Result<Option<String>> {
    let p = dir.join("chat_template.sha256.txt");
    if !p.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&p)?;
    Ok(Some(raw.trim().to_string()))
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

/// Fetch the OCI manifest JSON for `parsed` via `oras manifest fetch`.
/// Used by the trust-boundary code to read the cosign-covered
/// chat-template annotation without ever parsing the GGUF (per ADR-022).
fn run_oras_manifest_fetch(parsed: &ParsedRef) -> Result<serde_json::Value> {
    let out = Command::new("oras")
        .arg("manifest")
        .arg("fetch")
        .arg(parsed.canonical())
        .output()?;
    if !out.status.success() {
        return Err(PullError::ManifestFetchFailed {
            detail: format!(
                "exit {:?}: {}",
                out.status.code(),
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        });
    }
    serde_json::from_slice(&out.stdout).map_err(|e| PullError::ManifestFetchFailed {
        detail: format!("manifest is not valid JSON: {e}"),
    })
}

/// Pull the chat-template SHA-256 annotation out of an OCI manifest.
/// Returns `None` for non-model artifacts (devbox image, third-party
/// images that don't follow this convention). For artifacts whose
/// `artifactType` (or top-level `mediaType`) is in
/// [`MODEL_ARTIFACT_TYPES`] — i.e., one of the formats Aegis-Node's
/// `models-publish.yml` produces — the annotation is **required** and
/// a missing or malformed value is a hard refusal.
fn extract_chat_template_annotation(manifest: &serde_json::Value) -> Result<Option<String>> {
    // OCI 1.1+ manifests use `artifactType` for the artifact's purpose;
    // older single-blob artifacts ("config-as-content") put the same
    // value in `config.mediaType`. Accept either to interop with both
    // oras 1.1 and 1.2 outputs.
    let artifact_type = manifest
        .get("artifactType")
        .and_then(|v| v.as_str())
        .or_else(|| {
            manifest
                .get("config")
                .and_then(|c| c.get("mediaType"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("");
    let is_model = MODEL_ARTIFACT_TYPES.contains(&artifact_type);

    let annotation_value = manifest
        .get("annotations")
        .and_then(|a| a.get(CHAT_TEMPLATE_SHA_ANNOTATION))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    match annotation_value {
        Some(raw) => {
            if !is_valid_sha256_hex(&raw) {
                return Err(PullError::InvalidAnnotationValue {
                    annotation: CHAT_TEMPLATE_SHA_ANNOTATION,
                    value: raw,
                });
            }
            Ok(Some(raw))
        }
        None => {
            if is_model {
                Err(PullError::MissingChatTemplateAnnotation {
                    media_type: artifact_type.to_string(),
                    annotation: CHAT_TEMPLATE_SHA_ANNOTATION,
                })
            } else {
                Ok(None)
            }
        }
    }
}

fn is_valid_sha256_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
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
        if best.as_ref().is_none_or(|(s, _)| size > *s) {
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
    fn provenance_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let prov = Provenance {
            schema_version: PROVENANCE_SCHEMA_VERSION,
            oci_ref: "ghcr.io/x/m@sha256:abc".to_string(),
            manifest_digest: "abc".to_string(),
            blob_sha256: "deadbeef".to_string(),
            chat_template_sha256: Some("cafef00d".to_string()),
            cosign: CosignVerification {
                verified: true,
                mode: CosignMode::Keyless,
                key_path: None,
                keyless_identity_pattern: Some(".*".to_string()),
                keyless_oidc_issuer_pattern: Some(".*".to_string()),
            },
            pulled_at: "2026-05-07T00:00:00.000Z".to_string(),
        };
        write_provenance(dir.path(), &prov).expect("write");
        let read_back = read_provenance(dir.path()).expect("read").expect("present");
        assert_eq!(read_back, prov);
    }

    #[test]
    fn read_provenance_returns_none_for_legacy_cache() {
        let dir = tempfile::tempdir().unwrap();
        // No file written — simulates a cache entry from before the
        // sidecar was introduced.
        assert!(read_provenance(dir.path()).unwrap().is_none());
    }

    #[test]
    fn read_provenance_rejects_unknown_schema_version() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(PROVENANCE_FILENAME),
            r#"{"schema_version":99,"oci_ref":"x","manifest_digest":"x","blob_sha256":"x","chat_template_sha256":null,"cosign":{"verified":true,"mode":"keyless","key_path":null,"keyless_identity_pattern":".*","keyless_oidc_issuer_pattern":".*"},"pulled_at":"2026-05-07T00:00:00.000Z"}"#,
        )
        .unwrap();
        let err = read_provenance(dir.path()).unwrap_err();
        assert!(err.to_string().contains("schema_version=99"));
    }

    #[test]
    fn cosign_record_keyless_defaults_to_dotstar_patterns() {
        let cfg = PullConfig {
            cache_dir: PathBuf::from("/tmp"),
            cosign_key: None,
            keyless_identity: None,
            keyless_oidc_issuer: None,
        };
        let rec = cosign_record(&cfg);
        assert!(rec.verified);
        assert_eq!(rec.mode, CosignMode::Keyless);
        assert_eq!(rec.keyless_identity_pattern.as_deref(), Some(".*"));
        assert_eq!(rec.keyless_oidc_issuer_pattern.as_deref(), Some(".*"));
        assert!(rec.key_path.is_none());
    }

    #[test]
    fn cosign_record_keyed_passes_through_key_path() {
        let cfg = PullConfig {
            cache_dir: PathBuf::from("/tmp"),
            cosign_key: Some(PathBuf::from("/etc/cosign/team.pub")),
            keyless_identity: None,
            keyless_oidc_issuer: None,
        };
        let rec = cosign_record(&cfg);
        assert_eq!(rec.mode, CosignMode::Key);
        assert_eq!(rec.key_path.as_deref(), Some("/etc/cosign/team.pub"));
        assert!(rec.keyless_identity_pattern.is_none());
        assert!(rec.keyless_oidc_issuer_pattern.is_none());
    }

    #[test]
    fn rfc3339_now_has_canonical_shape() {
        let s = rfc3339_now();
        // YYYY-MM-DDTHH:MM:SS.mmmZ → 24 chars, ends with Z, has T at index 10.
        assert_eq!(s.len(), 24, "got: {s}");
        assert!(s.ends_with('Z'));
        assert_eq!(&s[10..11], "T");
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
