//! Liveness and version endpoints.
//!
//! `/healthz` is the lowest-overhead "is the server up" probe used
//! by the CLI's `aegis ui` smoke test and by the integration suite.
//! `/api/v1/version` is the first real `/api/v1/*` route — the
//! placeholder SPA calls it on load to prove the runtime is
//! reachable; subsequent surfaces use the same `/api/v1/` prefix.

use axum::extract::State;
use axum::Json;
use serde_json::json;

use crate::Config;

/// `GET /healthz` — returns `{"ok": true}`. Liveness only; carries
/// no version or feature data so it can be used by external probes
/// (k8s liveness, monitoring) without leaking build details.
pub async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "ok": true }))
}

/// `GET /api/v1/version` — returns the runtime's reported version,
/// the compiled-in feature set, and the bound listen address. The
/// SPA renders these in the placeholder header and (later) in the
/// Settings → About panel.
pub async fn version(State(config): State<Config>) -> Json<Config> {
    Json(config)
}
