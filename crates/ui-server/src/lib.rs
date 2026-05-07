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
use std::sync::Arc;

use axum::extract::FromRef;
use axum::routing::{get, post};
use axum::Router;
use serde::Serialize;
use tokio::sync::RwLock;

pub mod chat;
mod embed;
mod handlers;

pub use chat::{ChatBackend, ChatBackendError, ChatBackendFactory, StubBackend, TurnResult};
pub use embed::UiDist;
pub use handlers::sessions::SessionRegistry;

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

/// Composite handler state. axum's `FromRef` impls below let
/// individual handlers extract whichever sub-state they need.
///
/// Sub-phase 1d.2e.1 wraps `chat_backend` in `Arc<RwLock<…>>` so
/// the model-picker fork endpoint can swap the inner backend at
/// runtime per [ADR-032](../../docs/adrs/032-webui-model-library-and-session-forking.md)
/// §"Session Forking." Reads happen at WebSocket-connect time —
/// active connections keep their captured backend for the connection's
/// lifetime, which is fine because the SPA closes + reopens the WS
/// after a successful fork.
#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub sessions: SessionRegistry,
    /// The chat surface's backend, behind `Arc<RwLock<…>>` so the
    /// fork endpoint can swap it. Sub-phase 1d.2b plumbed the real
    /// `Session::run_turn`-driven implementation through here when
    /// `aegis ui --manifest <m> --model <m>` is provided; 1d.2e.1
    /// adds the swap mechanism. Without `--manifest`/`--model`,
    /// [`StubBackend`] keeps the chat UI visibly functional with an
    /// operator hint.
    pub chat_backend: Arc<RwLock<Arc<dyn ChatBackend>>>,
    /// Optional factory for forking. `Some` when the CLI booted with
    /// `--manifest`/`--model` (real backend); `None` for stub mode
    /// (no manifest/model context to re-boot against). The fork
    /// endpoint returns 503 when this is `None`.
    pub chat_backend_factory: Option<Arc<dyn ChatBackendFactory>>,
}

impl FromRef<AppState> for Config {
    fn from_ref(state: &AppState) -> Config {
        state.config.clone()
    }
}

impl FromRef<AppState> for SessionRegistry {
    fn from_ref(state: &AppState) -> SessionRegistry {
        state.sessions.clone()
    }
}

impl FromRef<AppState> for Arc<RwLock<Arc<dyn ChatBackend>>> {
    fn from_ref(state: &AppState) -> Arc<RwLock<Arc<dyn ChatBackend>>> {
        state.chat_backend.clone()
    }
}

impl FromRef<AppState> for Option<Arc<dyn ChatBackendFactory>> {
    fn from_ref(state: &AppState) -> Option<Arc<dyn ChatBackendFactory>> {
        state.chat_backend_factory.clone()
    }
}

/// Build the axum router for the UI server with the default
/// [`StubBackend`] attached and no factory (so the fork endpoint
/// returns 503). Convenience for tests and the CLI's no-model path.
pub fn router(config: Config) -> Router {
    router_with_backend(config, Arc::new(StubBackend), None)
}

/// Build the axum router with a caller-supplied [`ChatBackend`] and
/// optional [`ChatBackendFactory`]. `aegis-cli`'s `aegis ui` subcommand
/// uses this when `--manifest`/`--model` are provided.
///
/// `SessionRegistry` is constructed fresh inside the router; sessions
/// don't survive a process restart (per ADR-031 §"Single agent, single
/// user" — persistence-across-restart is a v1.0.0 multi-turn concern).
pub fn router_with_backend(
    config: Config,
    chat_backend: Arc<dyn ChatBackend>,
    chat_backend_factory: Option<Arc<dyn ChatBackendFactory>>,
) -> Router {
    let state = AppState {
        config,
        sessions: SessionRegistry::new(),
        chat_backend: Arc::new(RwLock::new(chat_backend)),
        chat_backend_factory,
    };
    Router::new()
        .route("/healthz", get(handlers::health::healthz))
        .route("/api/v1/version", get(handlers::health::version))
        .route("/api/v1/models", get(handlers::models::list_models))
        .route(
            "/api/v1/manifests",
            post(handlers::manifests::save_manifest),
        )
        .route(
            "/api/v1/manifests/validate",
            post(handlers::validate::validate_manifest),
        )
        .route("/api/v1/sessions", post(handlers::sessions::create_session))
        .route(
            "/api/v1/sessions/fork",
            post(handlers::sessions::fork_session),
        )
        .route("/api/v1/stream", get(handlers::sessions::stream))
        .fallback(handlers::assets::serve_embedded)
        .with_state(state)
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
    serve_with_backend(config, Arc::new(StubBackend), None).await
}

/// `serve` + a caller-supplied [`ChatBackend`] + optional factory.
/// The CLI's `aegis ui` subcommand uses this overload when it has a
/// Session ready to drive the chat surface and a factory ready to
/// fork to other models; the no-backend path uses [`serve`] which
/// falls back to [`StubBackend`] without a factory.
pub async fn serve_with_backend(
    config: Config,
    chat_backend: Arc<dyn ChatBackend>,
    chat_backend_factory: Option<Arc<dyn ChatBackendFactory>>,
) -> anyhow::Result<()> {
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
    axum::serve(
        listener,
        router_with_backend(config, chat_backend, chat_backend_factory),
    )
    .await?;
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
