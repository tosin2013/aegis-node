//! Manifest read/write handlers.
//!
//! Sub-phase 1d.1c ships the save side: the SPA's Manifest Builder
//! posts the editor's current YAML buffer here, the handler writes
//! it to a single draft slot under `~/.config/aegis/`, and the CLI
//! consumes the same file via `aegis run --manifest …`.
//!
//! ## Why one draft slot?
//!
//! Multi-manifest management with a directory listing UI lands in a
//! later sub-phase. For 1d.1c the workflow is "edit → save → run
//! against the saved file" which is what operators do on the CLI
//! today. A single well-known path keeps the surface tight.
//!
//! ## Validation gap
//!
//! This handler does **not** invoke `aegis validate` — the
//! validator is implemented in the Go binary (`cmd/aegis/`, ADR-002
//! split-language) and the cross-language plumbing is its own
//! sub-phase 1d.1d work item. Until then, operators run
//! `aegis validate ~/.config/aegis/manifests/draft.yaml` from the
//! CLI after saving. A future amendment will add
//! `POST /api/v1/manifests/validate` that shells out to the Go
//! validator and returns line-level diagnostics for inline rendering
//! in the Monaco editor.

use std::path::PathBuf;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

const DRAFT_FILENAME: &str = "draft.yaml";
const MAX_BODY_BYTES: usize = 1024 * 1024; // 1 MiB cap on the YAML body

#[derive(Debug, Serialize)]
pub struct SaveResponse {
    /// True when the write completed successfully. Always true in
    /// the OK path; included so the SPA can pattern-match on shape.
    pub saved: bool,
    /// Absolute path the YAML was written to. The SPA echoes this
    /// back to the operator so they can copy it into
    /// `aegis run --manifest …`.
    pub path: String,
    /// Number of bytes written. Lets the SPA show a "N bytes saved"
    /// confirmation without re-reading the file.
    pub bytes: usize,
}

/// `POST /api/v1/manifests` — save the request body as
/// `~/.config/aegis/manifests/draft.yaml`.
///
/// The body is treated as opaque YAML text. Validation is out of
/// scope for this handler (see module-level note); the operator
/// runs `aegis validate` from the CLI to lint the saved file.
pub async fn save_manifest(body: String) -> Response {
    if body.len() > MAX_BODY_BYTES {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "manifest body exceeds {} byte cap (got {} bytes)",
                MAX_BODY_BYTES,
                body.len()
            ),
        )
            .into_response();
    }

    let path = match draft_path() {
        Some(p) => p,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not resolve user config dir",
            )
                .into_response();
        }
    };

    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("create_dir_all {}: {e}", parent.display()),
            )
                .into_response();
        }
    }

    if let Err(e) = std::fs::write(&path, &body) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("write {}: {e}", path.display()),
        )
            .into_response();
    }

    let bytes = body.len();
    Json(SaveResponse {
        saved: true,
        path: path.display().to_string(),
        bytes,
    })
    .into_response()
}

/// Resolve `~/.config/aegis/manifests/draft.yaml` (or the
/// platform-equivalent config dir on macOS / Windows). Mirrors the
/// `dirs::config_dir` lookup used by `crates/cli/src/lib.rs::resolve_identity_dir`
/// so the WebUI's draft co-locates with the runtime config tree the
/// CLI already reads.
fn draft_path() -> Option<PathBuf> {
    let base = dirs::config_dir()?;
    Some(base.join("aegis").join("manifests").join(DRAFT_FILENAME))
}
