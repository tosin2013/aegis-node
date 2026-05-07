//! `GET /api/v1/models` — enumerate the local model cache.
//!
//! Per [ADR-032](../../../docs/adrs/032-webui-model-library-and-session-forking.md)
//! the WebUI's Model Library wraps `aegis pull`; this handler is
//! the read-side counterpart that the SPA calls on the `/models`
//! route to render the list of locally-cached artifacts.
//!
//! Cache layout (per `crates/cli/src/pull.rs::default_cache_dir`):
//!
//! ```text
//! ~/.cache/aegis/models/
//!   <sha256_hex>/
//!     blob.bin
//!     ref.txt                    (canonical OCI ref)
//!     sha256.txt                 (blob's actual SHA-256)
//!     chat_template.sha256.txt   (optional)
//!     provenance.json            (optional — present for pulls
//!                                 from sub-phase 1d.1e onward)
//! ```
//!
//! ### `oci_ref` resolution
//!
//! Two-tier fallback so the Models page surfaces a usable ref for
//! every cached model regardless of when it was pulled:
//!
//! 1. **`provenance.json`** — full record (oci_ref + cosign config +
//!    pulled_at timestamp). Written by `crates/cli/src/pull.rs::pull`
//!    after a successful pull.
//! 2. **`ref.txt`** — canonical OCI ref only. Predates the
//!    provenance sidecar; legacy cache entries always have it.
//!
//! If neither is present, the model is reported with `oci_ref: null`
//! (an unusual cache state — manually staged blob without sidecars).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use axum::Json;
use serde::{Deserialize, Serialize};

/// Schema version for the `provenance.json` sidecar this handler
/// reads. Mirrors `crates/cli/src/pull.rs::PROVENANCE_SCHEMA_VERSION`
/// — the format is the cross-crate contract. Bumping requires editing
/// both files in lockstep.
const PROVENANCE_SCHEMA_VERSION: u32 = 1;

/// Read-side mirror of `crates/cli/src/pull.rs::Provenance`. The two
/// types are intentionally duplicated rather than shared via a
/// crate dep — `aegis-cli` already depends on `aegis-ui-server`,
/// so the reverse would be a circular dep. The JSON schema is the
/// contract; field names + serde renames must match.
#[derive(Debug, Deserialize)]
struct ProvenanceFile {
    schema_version: u32,
    oci_ref: String,
    #[serde(default)]
    cosign: Option<CosignFile>,
    #[serde(default)]
    pulled_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CosignFile {
    verified: bool,
    mode: String,
    #[serde(default)]
    keyless_identity_pattern: Option<String>,
    #[serde(default)]
    keyless_oidc_issuer_pattern: Option<String>,
    #[serde(default)]
    key_path: Option<String>,
}

/// Cosign verification context surfaced to the SPA. Distilled from
/// the on-disk `provenance.json::cosign` block; the SPA renders
/// "verified · keyless ⟨pattern⟩" / "verified · key ⟨path⟩" badges.
#[derive(Debug, Serialize)]
pub struct ModelCosign {
    /// Always `true` when present — failed verifications never get
    /// written. Surfaced as a field so the SPA can pattern-match
    /// without inferring from absence.
    pub verified: bool,
    /// `"key"` or `"keyless"` per the validator's enum.
    pub mode: String,
    /// Identity-regex pattern that cosign accepted (keyless mode
    /// only). For operators reading the Model Library to confirm
    /// the constraint they configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyless_identity_pattern: Option<String>,
    /// OIDC-issuer regex pattern (keyless mode only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyless_oidc_issuer_pattern: Option<String>,
    /// Operator-supplied public key path (keyed mode only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Model {
    /// Directory name in the model cache — the SHA-256 of the
    /// artifact's manifest. Stable for the lifetime of the cached
    /// copy.
    pub digest: String,
    /// Original OCI reference the artifact was pulled with. Sourced
    /// from `provenance.json` when present, falling back to
    /// `ref.txt` for legacy cache entries. `null` only if neither
    /// sidecar exists (an unusual manually-staged-blob case).
    pub oci_ref: Option<String>,
    /// Cosign verification details from `provenance.json`. `None`
    /// for legacy cache entries that predate the sidecar.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cosign: Option<ModelCosign>,
    /// Total size of all files under the model's cache directory,
    /// in bytes.
    pub size_bytes: u64,
    /// RFC3339 timestamp of the cache directory's last-modified
    /// time. Approximates "last used" until we add explicit
    /// access-tracking sidecars.
    pub last_used: Option<String>,
    /// RFC3339 timestamp recorded in `provenance.json::pulled_at`,
    /// set by `aegis pull` at the moment cosign verification
    /// succeeded. Distinct from `last_used`: pulled_at is the
    /// supply-chain-relevant timestamp; last_used is the local
    /// access timestamp. `None` for legacy cache entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pulled_at: Option<String>,
    /// Whether a `chat_template.sha256.txt` sidecar is present
    /// (per ADR-022 / OCI-B). Indicates the F1 chat-template-binding
    /// extension is available for sessions booted against this model.
    pub has_chat_template: bool,
}

#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    /// Absolute path of the model cache root. Surfaced so the SPA
    /// can show operators where the artifacts live (and recognise
    /// when an override has been set).
    pub cache_dir: String,
    /// Cached models, ordered by digest for determinism.
    pub models: Vec<Model>,
}

/// `GET /api/v1/models`
pub async fn list_models() -> Json<ModelsResponse> {
    let cache_dir = match resolve_cache_dir() {
        Some(p) => p,
        None => {
            return Json(ModelsResponse {
                cache_dir: "(unresolvable)".to_string(),
                models: Vec::new(),
            });
        }
    };

    let mut models = match enumerate_cache(&cache_dir) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                target: "aegis_ui_server",
                err = %e,
                cache_dir = %cache_dir.display(),
                "failed to enumerate model cache; returning empty list",
            );
            Vec::new()
        }
    };
    models.sort_by(|a, b| a.digest.cmp(&b.digest));

    Json(ModelsResponse {
        cache_dir: cache_dir.display().to_string(),
        models,
    })
}

/// Mirrors `crates/cli/src/pull.rs::default_cache_dir` without the
/// dep cycle. Returns `None` when the OS doesn't expose a cache
/// dir (rare; treated as "no models cached").
fn resolve_cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("aegis").join("models"))
}

fn enumerate_cache(cache_dir: &Path) -> std::io::Result<Vec<Model>> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(cache_dir) {
        Ok(it) => it,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(digest) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // Cache subdirs are named by 64-char hex SHA-256; skip
        // anything else so stray files / non-cache directories
        // don't show up in the listing.
        if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }

        let size_bytes = dir_size_bytes(&path).unwrap_or(0);
        let last_used = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(format_rfc3339);
        let has_chat_template = path.join("chat_template.sha256.txt").is_file();
        let provenance = read_provenance(&path);
        let oci_ref = provenance
            .as_ref()
            .map(|p| p.oci_ref.clone())
            .or_else(|| read_ref_txt(&path));
        let pulled_at = provenance.as_ref().and_then(|p| p.pulled_at.clone());
        let cosign = provenance.and_then(|p| p.cosign.map(into_model_cosign));

        out.push(Model {
            digest: format!("sha256:{digest}"),
            oci_ref,
            cosign,
            size_bytes,
            last_used,
            pulled_at,
            has_chat_template,
        });
    }
    Ok(out)
}

/// Read + parse `provenance.json` from a cache subdir. Returns
/// `None` for legacy entries that don't have one OR for parse
/// failures (logged but non-fatal — a malformed sidecar shouldn't
/// hide the model from the listing). Schema-version mismatch
/// is also treated as "ignore"; the read-side type accepts only
/// the version it knows.
fn read_provenance(dir: &Path) -> Option<ProvenanceFile> {
    let path = dir.join("provenance.json");
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::warn!(
                target: "aegis_ui_server",
                err = %e,
                path = %path.display(),
                "reading provenance.json failed; falling back to ref.txt",
            );
            return None;
        }
    };
    let parsed: ProvenanceFile = match serde_json::from_slice(&bytes) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                target: "aegis_ui_server",
                err = %e,
                path = %path.display(),
                "provenance.json malformed; falling back to ref.txt",
            );
            return None;
        }
    };
    if parsed.schema_version != PROVENANCE_SCHEMA_VERSION {
        tracing::warn!(
            target: "aegis_ui_server",
            got = parsed.schema_version,
            expected = PROVENANCE_SCHEMA_VERSION,
            path = %path.display(),
            "provenance.json schema_version mismatch; falling back to ref.txt",
        );
        return None;
    }
    Some(parsed)
}

/// Read the legacy `ref.txt` sidecar — present on every cache entry
/// pulled via `aegis pull` regardless of when. Returns `None` if
/// the file is missing or unreadable.
fn read_ref_txt(dir: &Path) -> Option<String> {
    let path = dir.join("ref.txt");
    std::fs::read_to_string(&path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn into_model_cosign(c: CosignFile) -> ModelCosign {
    ModelCosign {
        verified: c.verified,
        mode: c.mode,
        keyless_identity_pattern: c.keyless_identity_pattern,
        keyless_oidc_issuer_pattern: c.keyless_oidc_issuer_pattern,
        key_path: c.key_path,
    }
}

fn dir_size_bytes(dir: &Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current)?.flatten() {
            let meta = entry.metadata()?;
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                total = total.saturating_add(meta.len());
            }
        }
    }
    Ok(total)
}

/// Format a `SystemTime` as an RFC3339 timestamp without pulling in
/// the `chrono` crate. ui-server stays light on dependencies; the
/// SPA reformats the timestamp client-side anyway.
fn format_rfc3339(t: SystemTime) -> Option<String> {
    let dur = t.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    let secs = dur.as_secs() as i64;
    let nanos = dur.subsec_nanos();
    Some(naive_rfc3339_from_unix(secs, nanos))
}

/// Pure-stdlib UNIX-seconds → RFC3339 (`YYYY-MM-DDTHH:MM:SS.fffZ`).
/// Calendar arithmetic uses the proleptic Gregorian calendar so
/// dates after 1970 map cleanly. Adequate for "last modified"
/// timestamps; the SPA does its own locale-aware rendering.
fn naive_rfc3339_from_unix(secs: i64, nanos: u32) -> String {
    const SECONDS_PER_DAY: i64 = 86_400;
    let days = secs.div_euclid(SECONDS_PER_DAY);
    let time_of_day = secs.rem_euclid(SECONDS_PER_DAY);
    let hh = time_of_day / 3600;
    let mm = (time_of_day % 3600) / 60;
    let ss = time_of_day % 60;

    // Convert days-since-1970 to (year, month, day) via Howard
    // Hinnant's algorithm — see http://howardhinnant.github.io/date_algorithms.html
    // (`civil_from_days`). Public domain.
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
