//! `aegis ui` subcommand: start the Phase 1d Community UI server.
//!
//! Per [ADR-031](../../../docs/adrs/031-community-webui-for-local-collaboration.md)
//! the UI is a static SPA bundled into the binary, served by an
//! axum process bound to a loopback address. This subcommand is the
//! sub-phase 1d.0 entry point — it boots the server and serves the
//! placeholder `ui/dist/` payload.
//!
//! Sub-phase 1d.2 ([docs/plans/v0.9.5-ui-implementation.md](../../../docs/plans/v0.9.5-ui-implementation.md))
//! will fold this into `aegis run --ui` once the chat surface ties
//! to a `Session::run_turn` loop. Until then `aegis ui` runs
//! standalone — no manifest, no model required, only the static
//! placeholder + `/api/v1/version`.
//!
//! Ctrl-C cleanly shuts the server down.

use std::net::SocketAddr;

use aegis_ui_server::Config;
use anyhow::{Context, Result};
use clap::Args;

#[derive(Debug, Args)]
pub struct UiArgs {
    /// Address to bind. Must be a loopback address (`127.0.0.0/8`
    /// or `::1`) per [ADR-031](../../../docs/adrs/031-community-webui-for-local-collaboration.md);
    /// non-loopback values are refused at bind time, not just here.
    #[arg(long, default_value = "127.0.0.1:7777")]
    pub listen: SocketAddr,
}

/// Entry-point invoked from `aegis_cli::run`.
pub fn execute(args: UiArgs) -> Result<()> {
    // Pre-validate the loopback constraint so the "listening on …"
    // banner doesn't print before a bind error is about to fire.
    // `serve()` enforces the same rule again — this is just UX.
    if !args.listen.ip().is_loopback() {
        anyhow::bail!(
            "refusing to bind {}: ADR-031 requires the Community UI listen on a \
             loopback address (127.0.0.0/8 or ::1). Operators who want network-reachable \
             UI deploy the Enterprise UI per ADR-034.",
            args.listen,
        );
    }

    let config = Config {
        version: env!("CARGO_PKG_VERSION").to_string(),
        features: compiled_features(),
        listen: args.listen,
    };

    eprintln!(
        "Aegis-Node Community UI listening on http://{}",
        args.listen
    );
    eprintln!("Open this URL in your browser; press Ctrl-C to stop.");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("building tokio runtime for ui-server")?;

    runtime.block_on(async move {
        tokio::select! {
            res = aegis_ui_server::serve(config) => res,
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\nshutting down");
                Ok(())
            }
        }
    })
}

/// Names of the optional Cargo features the CLI was compiled with.
/// Surfaced in `/api/v1/version` so the future Model Library UI can
/// warn before pulling an artifact whose backend isn't available.
fn compiled_features() -> Vec<String> {
    let mut v = Vec::new();
    if cfg!(feature = "llama") {
        v.push("llama".to_string());
    }
    if cfg!(feature = "litertlm") {
        v.push("litertlm".to_string());
    }
    v
}
