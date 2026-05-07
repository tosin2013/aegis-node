//! `aegis ui` subcommand: start the Phase 1d Community UI server.
//!
//! Per [ADR-031](../../../docs/adrs/031-community-webui-for-local-collaboration.md)
//! the UI is a static SPA bundled into the binary, served by an
//! axum process bound to a loopback address. Sub-phase 1d.2b adds
//! optional `--manifest` / `--model` / `--backend` flags so the
//! chat surface drives a real `Session::run_turn` against a loaded
//! inference engine. Without those flags the server still runs and
//! the chat surface falls back to a stub-echo backend (per
//! [`aegis_ui_server::StubBackend`]) that explains how to attach a
//! model.
//!
//! Ctrl-C cleanly shuts the server down.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use aegis_inference_engine::Session;
use aegis_ui_server::{ChatBackend, ChatBackendError, Config, StubBackend, TurnResult};
use anyhow::{Context, Result};
use clap::Args;

use crate::run::{boot_session_for_ui, BackendKind};

#[derive(Debug, Args)]
pub struct UiArgs {
    /// Address to bind. Must be a loopback address (`127.0.0.0/8`
    /// or `::1`) per [ADR-031](../../../docs/adrs/031-community-webui-for-local-collaboration.md);
    /// non-loopback values are refused at bind time, not just here.
    #[arg(long, default_value = "127.0.0.1:7777")]
    pub listen: SocketAddr,

    /// Path to the manifest YAML the chat surface should bind to.
    /// When provided alongside `--model`, `aegis ui` boots a
    /// `Session` at startup and the chat surface drives a real
    /// `Session::run_turn`. Omitting either value keeps the chat
    /// surface on [`StubBackend`] echo with an operator hint.
    #[arg(long)]
    pub manifest: Option<PathBuf>,

    /// Path to the model artifact (per `aegis pull`'s cache layout
    /// — typically `~/.cache/aegis/models/<digest>/blob.bin`).
    #[arg(long)]
    pub model: Option<PathBuf>,

    /// Which inference backend powers the chat surface. Mirrors
    /// `aegis run --backend`. Each requires the matching Cargo
    /// feature at build time.
    #[arg(long, default_value = "llama")]
    pub backend: BackendKind,

    /// Optional runtime config file (digest source) — same role as
    /// `aegis run --config`.
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Optional `chat_template.sha256.txt` sidecar produced by
    /// `aegis pull` (per ADR-022 / OCI-B).
    #[arg(long)]
    pub chat_template_sidecar: Option<PathBuf>,

    /// Where the local CA's `ca.crt`/`ca.key` live. Default:
    /// `$XDG_CONFIG_HOME/aegis/identity`.
    #[arg(long)]
    pub identity_dir: Option<PathBuf>,

    /// SPIFFE workload-name segment.
    #[arg(long, default_value = "ui")]
    pub workload: String,

    /// SPIFFE instance segment.
    #[arg(long, default_value = "inst-1")]
    pub instance: String,

    /// Output JSONL ledger path. Defaults to
    /// `./ledger-ui-<session-id>.jsonl`.
    #[arg(long)]
    pub ledger: Option<PathBuf>,
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

    let chat_backend = build_chat_backend(&args)?;

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
            res = aegis_ui_server::serve_with_backend(config, chat_backend) => res,
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\nshutting down");
                Ok(())
            }
        }
    })
}

/// Resolve the chat backend based on the CLI args. Returns the real
/// [`SessionBackend`] when both `--manifest` and `--model` are
/// provided AND the matching Cargo feature is built; otherwise a
/// [`StubBackend`] so the chat surface stays visibly functional with
/// an operator hint.
///
/// Half-set inputs (one of `--manifest`/`--model` provided but not
/// the other) are a usage error — the operator clearly meant to
/// attach a Session, so failing fast is friendlier than silently
/// falling back to the stub.
fn build_chat_backend(args: &UiArgs) -> Result<Arc<dyn ChatBackend>> {
    match (&args.manifest, &args.model) {
        (Some(manifest), Some(model)) => {
            eprintln!(
                "booting Session against manifest={} model={} backend={:?}",
                manifest.display(),
                model.display(),
                args.backend,
            );
            let session = boot_session_for_ui(
                manifest.clone(),
                model.clone(),
                args.config.clone(),
                args.chat_template_sidecar.clone(),
                args.identity_dir.clone(),
                args.workload.clone(),
                args.instance.clone(),
                args.ledger.clone(),
                args.backend,
            )
            .context("booting Session for chat surface")?;
            Ok(Arc::new(SessionBackend::new(session)))
        }
        (None, None) => {
            eprintln!(
                "no --manifest/--model provided — chat surface uses StubBackend (echo + hint)",
            );
            Ok(Arc::new(StubBackend))
        }
        (Some(_), None) | (None, Some(_)) => anyhow::bail!(
            "--manifest and --model must be provided together. Omit both for the stub backend, \
             or pass both for real Session::run_turn integration.",
        ),
    }
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

/// `ChatBackend` impl that wraps a real
/// [`aegis_inference_engine::Session`]. The Session is held behind a
/// `Mutex` because `Session::run_turn` needs `&mut self` and multiple
/// WebSocket connections may share the chat surface — the Mutex
/// serializes turns, which is fine for v0.9.5's single-user posture
/// (one operator at the keyboard at a time per ADR-031).
///
/// Future v1.0.0 multi-turn work (ADRs 025–030) brings per-turn
/// circuit breakers and aggregate quotas; both compose cleanly with
/// the lock-around-Session pattern.
struct SessionBackend {
    inner: Arc<Mutex<Session>>,
}

impl SessionBackend {
    fn new(session: Session) -> Self {
        Self {
            inner: Arc::new(Mutex::new(session)),
        }
    }
}

impl ChatBackend for SessionBackend {
    fn run_turn(&self, prompt: &str) -> Result<TurnResult, ChatBackendError> {
        let mut session = self
            .inner
            .lock()
            .map_err(|e| ChatBackendError::new(format!("session mutex poisoned: {e}")))?;

        let outcome = session
            .run_turn(prompt)
            .map_err(|e| ChatBackendError::new(format!("Session::run_turn: {e}")))?;

        let summaries = outcome
            .tool_calls
            .iter()
            .map(|call| format_tool_call_summary(&call.name, &call.result))
            .collect();

        Ok(TurnResult {
            assistant_text: outcome.assistant_text,
            tool_call_summaries: summaries,
        })
    }
}

/// Render one tool-call outcome as a human-readable single line.
/// Mirrors the rendering logic in `aegis run --prompt`; sub-phase
/// 1d.2c will replace this with structured frames the SPA renders
/// as inline cards with the gate decision.
fn format_tool_call_summary(name: &str, result: &aegis_inference_engine::ToolCallResult) -> String {
    use aegis_inference_engine::ToolCallResult;
    match result {
        ToolCallResult::Success(_) => format!("{name} → success"),
        ToolCallResult::Denied(reason) => format!("{name} → DENIED: {reason}"),
        ToolCallResult::RequiresApproval(reason) => {
            format!("{name} → REQUIRES_APPROVAL: {reason}")
        }
        ToolCallResult::Unroutable(reason) => format!("{name} → UNROUTABLE: {reason}"),
    }
}
