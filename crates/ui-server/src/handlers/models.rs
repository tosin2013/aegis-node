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
//!     <model artifact files>
//!     chat_template.sha256.txt   (optional sidecar)
//! ```
//!
//! Sub-phase 1d.1b ships read-only enumeration only. The "Add model"
//! flow that wraps `pull::pull` over WebSocket lands in 1d.1c.
//!
//! ### Future-work notes
//!
//! - The OCI ref the operator originally pulled with isn't preserved
//!   in the cache today (only the digest is). 1d.1c will add a
//!   `provenance.json` sidecar capturing the source ref + cosign
//!   verification result so this handler can surface
//!   `oci_ref` non-null. Until then, `oci_ref` is always null.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use axum::Json;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Model {
    /// Directory name in the model cache — the SHA-256 of the
    /// artifact's manifest. Stable for the lifetime of the cached
    /// copy.
    pub digest: String,
    /// Original OCI reference the artifact was pulled with. Always
    /// `null` in 1d.1b; 1d.1c populates this from a `provenance.json`
    /// sidecar.
    pub oci_ref: Option<String>,
    /// Total size of all files under the model's cache directory,
    /// in bytes.
    pub size_bytes: u64,
    /// RFC3339 timestamp of the cache directory's last-modified
    /// time. Approximates "last used" until we add explicit
    /// access-tracking sidecars.
    pub last_used: Option<String>,
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

        out.push(Model {
            digest: format!("sha256:{digest}"),
            oci_ref: None,
            size_bytes,
            last_used,
            has_chat_template,
        });
    }
    Ok(out)
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
