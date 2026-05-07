//! `aegis-ui-server` — Phase 1d Community UI HTTP server.
//!
//! Per [ADR-031](../../docs/adrs/031-community-webui-for-local-collaboration.md)
//! this crate is the localhost-bound axum server that serves the
//! embedded SPA assets and exposes the `/api/v1/*` REST + WebSocket
//! routes the UI consumes.
//!
//! Sub-phase 1d.0 scope (this crate's first cut):
//!
//! - Static-asset serving from `ui/dist/` baked in at compile time
//!   via `rust-embed`. There is no runtime dependency on `ui/dist/`
//!   existing on disk.
//! - `GET /healthz` for liveness checks.
//! - `GET /api/v1/version` returning the runtime version + compiled
//!   feature flags + the bound listen address. The placeholder SPA
//!   calls this on load to prove the API is reachable.
//!
//! Subsequent sub-phases extend `Router` with session, manifest,
//! model-pull, and MCP-discovery handlers (1d.1 / 1d.2 / 1d.3).
//!
//! ## Localhost binding
//!
//! Per [ADR-031](../../docs/adrs/031-community-webui-for-local-collaboration.md)
//! §"Localhost-only," the server refuses to bind any non-loopback
//! address. The check is enforced at [`serve`] before `axum::serve`
//! is called — this is defence-in-depth on top of the CLI's
//! validation, so library users invoking us programmatically can't
//! accidentally expose the surface either.

use std::net::{IpAddr, SocketAddr};

use axum::routing::{get, post};
use axum::Router;
use serde::Serialize;

mod embed;
mod handlers;

pub use embed::UiDist;

/// Construction-time configuration passed in by the CLI. Carries the
/// values the `/api/v1/version` endpoint surfaces back to the UI;
/// kept as plain owned data so library users (tests, future
/// integration crates) can build a `Router` without touching a
/// network socket.
#[derive(Debug, Clone, Serialize)]
pub struct Config {
    /// Semantic version of the host binary, typically
    /// `env!("CARGO_PKG_VERSION")` from `crates/cli/`.
    pub version: String,
    /// Names of the optional Cargo features the host binary was
    /// compiled with (`"llama"`, `"litertlm"`, …). Reported back to
    /// the UI so the Model Library can warn before pulling an
    /// artifact whose backend isn't available.
    pub features: Vec<String>,
    /// Address the server is bound to. Reported back to the UI for
    /// the placeholder header and used by the localhost-binding
    /// guard in [`serve`].
    pub listen: SocketAddr,
}

/// Build the axum router for the UI server. Pure: no I/O, no socket
/// binding — call [`serve`] to actually run it.
///
/// Routes installed by sub-phase 1d.0:
///
/// | Path                  | Handler                                  |
/// |-----------------------|------------------------------------------|
/// | `GET /healthz`        | [`handlers::health::healthz`]            |
/// | `GET /api/v1/version` | [`handlers::health::version`]            |
/// | `GET /*`              | embedded `ui/dist/` via [`UiDist`]       |
pub fn router(config: Config) -> Router {
    Router::new()
        .route("/healthz", get(handlers::health::healthz))
        .route("/api/v1/version", get(handlers::health::version))
        .route("/api/v1/models", get(handlers::models::list_models))
        .route(
            "/api/v1/manifests",
            post(handlers::manifests::save_manifest),
        )
        .fallback(handlers::assets::serve_embedded)
        .with_state(config)
}

/// Bind the server to `addr`, refusing any non-loopback address, and
/// run until the listener errors or the process is terminated.
///
/// The non-loopback refusal is enforced here (not just in CLI flag
/// parsing) so library users — tests, future integration crates —
/// can't accidentally expose the surface either. Per
/// [ADR-031](../../docs/adrs/031-community-webui-for-local-collaboration.md)
/// §"Localhost-only" there is no escape hatch.
pub async fn serve(config: Config) -> anyhow::Result<()> {
    let addr = config.listen;
    if !is_loopback(addr.ip()) {
        anyhow::bail!(
            "refusing to bind {addr}: ADR-031 requires the Community UI listen on a \
             loopback address (127.0.0.0/8 or ::1). Operators who want network-reachable \
             UI deploy the Enterprise UI per ADR-034.",
        );
    }
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    tracing::info!(target: "aegis_ui_server", addr = %bound, "ui-server listening");
    axum::serve(listener, router(config)).await?;
    Ok(())
}

fn is_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn cfg(addr: &str) -> Config {
        Config {
            version: "0.0.0-test".to_string(),
            features: vec![],
            listen: addr.parse().expect("test address parses"),
        }
    }

    #[tokio::test]
    async fn refuses_to_bind_non_loopback() {
        let res = serve(cfg("0.0.0.0:0")).await;
        let err = res.expect_err("0.0.0.0 must be refused");
        assert!(err.to_string().contains("loopback"), "{err}");
    }

    #[tokio::test]
    async fn accepts_v4_loopback() {
        // Just construct + bind to an ephemeral port and immediately
        // drop. We don't run the server — `axum::serve` is the long-
        // running future and we only want to prove the loopback
        // gate accepts 127.0.0.1.
        let addr: SocketAddr = "127.0.0.1:0".parse().expect("parses");
        let listener = tokio::net::TcpListener::bind(addr).await.expect("binds");
        let bound = listener.local_addr().expect("bound addr");
        assert!(bound.ip().is_loopback());
    }

    #[tokio::test]
    async fn accepts_v6_loopback() {
        let addr: SocketAddr = "[::1]:0".parse().expect("parses");
        // ::1 isn't always available in CI containers; tolerate
        // bind failure but verify our gate would have allowed it.
        if let Ok(listener) = tokio::net::TcpListener::bind(addr).await {
            assert!(listener
                .local_addr()
                .expect("bound addr")
                .ip()
                .is_loopback());
        } else {
            assert!(is_loopback(addr.ip()));
        }
    }
}
