//! `aegis run` subcommand: boot a session, iterate a fixed tool-call
//! script, shutdown, print the chain root hash.
//!
//! Phase 1a (issue #28). The script JSON shape is:
//!
//! ```json
//! {
//!   "calls": [
//!     {"kind": "filesystem_read", "path": "/data/in.csv", "reasoning_step_id": "r1"},
//!     {"kind": "filesystem_write", "path": "/data/out.csv", "contents": "hello"},
//!     {"kind": "filesystem_delete", "path": "/tmp/scratch"},
//!     {"kind": "network_outbound", "host": "127.0.0.1", "port": 8080,
//!      "protocol": "tcp", "reasoning_step_id": "r4"},
//!     {"kind": "exec", "program": "/usr/bin/git", "args": ["status"]}
//!   ]
//! }
//! ```
//!
//! Halt semantics:
//!
//! - Identity rebind violation (model/manifest/config changed under us)
//!   → halt; the violation is already in the ledger.
//! - RequireApproval → halt for now. F0-D (#27) wires actual approval
//!   routing; until then, any approval-required action is treated as a
//!   hard refusal so the session doesn't proceed without consent.
//! - Plain Deny → record and continue; the agent saw it can't and the
//!   ledger has the Violation entry.
//! - Other I/O errors → halt and propagate.

use std::path::{Path, PathBuf};

use aegis_inference_engine::{BootConfig, Error as RtError, Session};
use aegis_policy::NetworkProto;
use anyhow::{Context, Result};
use clap::{Args, ValueEnum};
use serde::Deserialize;

/// Which inference backend `aegis run --prompt` uses for one
/// model-driven turn. Both choices require their respective Cargo
/// feature to be enabled at build time; selecting a backend whose
/// feature is off bails with an explicit "rebuild with --features X"
/// error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
#[value(rename_all = "lowercase")]
pub enum BackendKind {
    /// llama.cpp + GGUF model files (per ADR-014). Default for
    /// backwards compatibility — pre-LiteRT-B `aegis run --prompt`
    /// always used llama.cpp.
    #[default]
    Llama,
    /// LiteRT-LM + `.litertlm` model files (per ADR-023). Phase 1
    /// is CPU + greedy only; `temperature > 0.0` is refused at
    /// session boot.
    Litertlm,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    /// Path to the manifest YAML to enforce.
    #[arg(long)]
    pub manifest: PathBuf,

    /// Path to the model artifact (digest source — the bytes are not
    /// loaded in Phase 1a).
    #[arg(long)]
    pub model: PathBuf,

    /// Optional runtime config file (digest source).
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Optional `chat_template.sha256.txt` sidecar produced by
    /// `aegis pull` (per ADR-022 / OCI-B). When set, the digest is
    /// bound into the SVID via the `CHAT_TEMPLATE_BINDING_OID`
    /// extension and surfaced in the SessionStart ledger entry.
    #[arg(long)]
    pub chat_template_sidecar: Option<PathBuf>,

    /// Where the local CA's `ca.crt`/`ca.key` live. Default:
    /// `$XDG_CONFIG_HOME/aegis/identity`.
    #[arg(long)]
    pub identity_dir: Option<PathBuf>,

    /// SPIFFE workload-name segment.
    #[arg(long, default_value = "default")]
    pub workload: String,

    /// SPIFFE instance segment.
    #[arg(long, default_value = "inst-1")]
    pub instance: String,

    /// Output JSONL ledger path. Defaults to `./ledger-<session-id>.jsonl`.
    #[arg(long)]
    pub ledger: Option<PathBuf>,

    /// Caller-supplied session ID. Defaults to a UUIDv7-shaped string.
    #[arg(long)]
    pub session_id: Option<String>,

    /// JSON file with a fixed tool-call sequence (script-mode runs).
    /// Mutually exclusive with `--prompt`: a session is driven by
    /// either a deterministic script (no model) or a real model
    /// (LLM-B `Session::run_turn`), never both.
    #[arg(long, conflicts_with = "prompt")]
    pub script: Option<PathBuf>,

    /// User message to feed the LLM-B backend for one model-driven
    /// turn. Mutually exclusive with `--script`. The manifest's
    /// `inference.determinism` block (if any) flows through to the
    /// chosen backend's sampler — pinning seed + temperature 0.0
    /// yields byte-identical output, which is what the recorded
    /// demo program (ADR-020) depends on.
    ///
    /// Pair with `--backend` to pick the inference backend. The
    /// CLI must have been built with the matching Cargo feature
    /// (`llama` for the default `--backend llama`; `litertlm` for
    /// `--backend litertlm`).
    #[arg(long, conflicts_with = "script")]
    pub prompt: Option<String>,

    /// Inference backend used for `--prompt`. Default: `llama`
    /// (preserves pre-LiteRT-B behavior). `litertlm` is the
    /// LiteRT-LM backend per ADR-023; Phase 1 is CPU + greedy only,
    /// so the manifest's `inference.determinism.temperature` must
    /// be `0.0` (`omitted` is also fine — defaults to `0.0`).
    #[arg(long, value_enum, default_value_t = BackendKind::default())]
    pub backend: BackendKind,
}

/// Result of an `aegis run` invocation. Used by tests to assert
/// post-conditions without scraping stdout.
#[derive(Debug, Clone)]
pub struct RunOutcome {
    pub session_id: String,
    pub ledger_path: PathBuf,
    pub root_hash_hex: String,
    pub entry_count: u64,
    pub halted: bool,
    pub halt_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Script {
    calls: Vec<Call>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Call {
    FilesystemRead {
        path: PathBuf,
        #[serde(default)]
        reasoning_step_id: Option<String>,
    },
    FilesystemWrite {
        path: PathBuf,
        #[serde(default)]
        contents: String,
        #[serde(default)]
        reasoning_step_id: Option<String>,
    },
    FilesystemDelete {
        path: PathBuf,
        #[serde(default)]
        reasoning_step_id: Option<String>,
    },
    NetworkOutbound {
        host: String,
        port: u16,
        #[serde(default = "default_protocol")]
        protocol: String,
        #[serde(default)]
        reasoning_step_id: Option<String>,
    },
    Exec {
        program: PathBuf,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        reasoning_step_id: Option<String>,
    },
}

fn default_protocol() -> String {
    "tcp".to_string()
}

pub fn execute(args: RunArgs) -> Result<RunOutcome> {
    if args.script.is_none() && args.prompt.is_none() {
        anyhow::bail!("aegis run requires either --script <path> or --prompt <text>");
    }

    let session_id = args
        .session_id
        .clone()
        .unwrap_or_else(|| format!("session-{}", uuid_v7_like()));
    let ledger_path = args
        .ledger
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("ledger-{session_id}.jsonl")));
    let identity_dir = match &args.identity_dir {
        Some(p) => p.clone(),
        None => default_identity_dir()?,
    };

    let cfg = BootConfig {
        session_id: session_id.clone(),
        manifest_path: args.manifest.clone(),
        model_path: args.model.clone(),
        config_path: args.config.clone(),
        chat_template_sidecar: args.chat_template_sidecar.clone(),
        identity_dir,
        workload_name: args.workload.clone(),
        instance: args.instance.clone(),
        ledger_path: ledger_path.clone(),
    };
    let mut session = Session::boot(cfg).context("boot")?;

    // F3 file-channel approval (per ADR-005 / issue #27): if the env
    // var AEGIS_APPROVAL_FILE is set, attach a FileApprovalChannel
    // pointed at it. Without the env var, RequireApproval keeps the
    // legacy halt behavior — script continues, halt-on-RequireApproval
    // surfaces in dispatch() as a Halt::Stop. TTY channel and signed-
    // API channel are filed as separate issues (#35, #36).
    if let Ok(approval_path) = std::env::var("AEGIS_APPROVAL_FILE") {
        let channel = aegis_approval_gate::FileApprovalChannel::new(approval_path);
        session = session.with_approval_channel(Box::new(channel));
    }

    // Attach a stdio MCP client when the manifest declares any
    // `tools.mcp[]` servers (per ADR-018). Without this, the
    // mediator denies every MCP tool call with "no mcp client
    // configured for session" — fine for script-mode runs that
    // don't use MCP at all, but a regression for `--prompt` against
    // a manifest that grants MCP servers. Each `server_uri` is a
    // `stdio:/path/to/binary [args...]` URI per ADR-018; the client
    // spawns the child process on first invocation per server.
    if !session.policy().manifest().tools.mcp.is_empty() {
        let mcp_client = aegis_mcp_client::StdioMcpClient::new();
        session = session.with_mcp_client(Box::new(mcp_client));
    }

    let (halted, halt_reason) = match (args.prompt.as_ref(), args.script.as_ref()) {
        (Some(prompt), _) => run_prompt(&mut session, &args, prompt)?,
        (None, Some(script_path)) => run_script(&mut session, script_path)?,
        // The if-bail above ruled out both-None; matched here for an
        // exhaustive pattern that doesn't trip clippy::expect_used.
        (None, None) => unreachable!(),
    };

    let root_hash = session.shutdown().context("shutdown")?;
    let summary = aegis_ledger_writer::verify_file(&ledger_path).context("post-run verify")?;

    Ok(RunOutcome {
        session_id,
        ledger_path,
        root_hash_hex: hex::encode(root_hash),
        entry_count: summary.entry_count,
        halted,
        halt_reason,
    })
}

/// Drive a session via a fixed script (the original `aegis run` path —
/// deterministic, no model required, no LLM-B backend needed).
fn run_script(session: &mut Session, script_path: &Path) -> Result<(bool, Option<String>)> {
    let script = load_script(script_path)?;
    let mut halted = false;
    let mut halt_reason: Option<String> = None;
    for call in script.calls {
        match dispatch(session, call) {
            Ok(()) => {}
            Err(Halt::Continue) => {}
            Err(Halt::Stop(reason)) => {
                halted = true;
                halt_reason = Some(reason);
                break;
            }
            Err(Halt::Fatal(err)) => return Err(err),
        }
    }
    Ok((halted, halt_reason))
}

/// Drive a session via one model-driven turn (LLM-B `Session::run_turn`).
/// Dispatches on `args.backend` to either the llama.cpp wrapper
/// (ADR-014) or the LiteRT-LM wrapper (ADR-023). Each is gated on
/// its respective Cargo feature; choosing a backend whose feature
/// is off bails with an explicit "rebuild with --features X" error.
fn run_prompt(
    session: &mut Session,
    args: &RunArgs,
    prompt: &str,
) -> Result<(bool, Option<String>)> {
    use aegis_inference_engine::ToolCallResult;

    let loaded = match args.backend {
        BackendKind::Llama => load_llama_backend(session, &args.model)?,
        BackendKind::Litertlm => load_litertlm_backend(session, &args.model)?,
    };

    // Plug the loaded model into the session for the duration of the
    // turn. `set_loaded_model` is the &mut-self counterpart of
    // `Session::with_loaded_model`.
    session.set_loaded_model(loaded);

    let outcome = session.run_turn(prompt).context("run_turn")?;

    // Print the outcome for the user (and for the demo .tape recorder
    // — these lines are what the GIF shows).
    if let Some(text) = &outcome.assistant_text {
        println!("# assistant");
        println!("{text}");
    }
    for (i, call) in outcome.tool_calls.iter().enumerate() {
        match &call.result {
            ToolCallResult::Success(value) => {
                println!("# tool[{i}] {} → success", call.name);
                let pretty =
                    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
                println!("{pretty}");
            }
            ToolCallResult::Denied(reason) => {
                println!("# tool[{i}] {} → DENIED: {reason}", call.name);
            }
            ToolCallResult::RequiresApproval(reason) => {
                println!("# tool[{i}] {} → REQUIRES_APPROVAL: {reason}", call.name);
            }
            ToolCallResult::Unroutable(reason) => {
                println!("# tool[{i}] {} → UNROUTABLE: {reason}", call.name);
            }
        }
    }

    // The model-driven path doesn't surface a "halted" condition the
    // way the script path does — denials and approval refusals are
    // captured into the TurnOutcome rather than propagated. We return
    // `halted: false` so the caller can still check the ledger for
    // Violation entries via `aegis verify`.
    Ok((false, None))
}

#[cfg(feature = "llama")]
fn load_llama_backend(
    session: &Session,
    model_path: &Path,
) -> Result<Box<dyn aegis_inference_engine::LoadedModel>> {
    use aegis_inference_engine::Backend as _;
    use aegis_llama_backend::{
        Backend as LlamaBackend, DeterminismKnobs, LlamaCppBackend, SessionOptions,
    };
    use std::sync::Arc;

    // Construct LLM-A backend + LLM-B LlamaCppBackend wrapper. The
    // FFI is process-global (per ADR-014); we hold it for the duration
    // of this command and let it drop on exit.
    let llama_backend = Arc::new(LlamaBackend::init().context("llama backend init")?);

    // Resolve `inference.determinism` from the manifest. The Policy
    // already parsed it; pull it back out via the public accessor.
    let determinism = session
        .policy()
        .manifest()
        .inference
        .as_ref()
        .and_then(|i| i.determinism.as_ref())
        .map(DeterminismKnobs::from)
        .unwrap_or_default();

    let options = SessionOptions {
        determinism,
        ..SessionOptions::default()
    };

    let cpp_backend = LlamaCppBackend::new(llama_backend, options);
    cpp_backend
        .load(model_path)
        .with_context(|| format!("loading model {}", model_path.display()))
}

#[cfg(not(feature = "llama"))]
fn load_llama_backend(
    _session: &Session,
    _model_path: &Path,
) -> Result<Box<dyn aegis_inference_engine::LoadedModel>> {
    anyhow::bail!(
        "aegis run --backend llama requires the 'llama' Cargo feature; rebuild with \
         `cargo install --path crates/cli --features llama` (per ADR-014)"
    );
}

#[cfg(feature = "litertlm")]
fn load_litertlm_backend(
    session: &Session,
    model_path: &Path,
) -> Result<Box<dyn aegis_inference_engine::LoadedModel>> {
    use aegis_inference_engine::Backend as _;
    use aegis_litertlm_backend::{DeterminismKnobs, LiteRtLmBackend, SessionOptions};

    // Resolve `inference.determinism` from the manifest, same shape
    // as the llama path. The two backends share the manifest schema
    // (per ADR-023 §"Determinism + replay") so the conversion is a
    // straight `From` impl.
    let determinism = session
        .policy()
        .manifest()
        .inference
        .as_ref()
        .and_then(|i| i.determinism.as_ref())
        .map(DeterminismKnobs::from)
        .unwrap_or_default();

    let options = SessionOptions {
        determinism,
        ..SessionOptions::default()
    };

    // LiteRT-LM doesn't require a process-global init step (unlike
    // llama.cpp); the backend wrapper is constructed and loaded in
    // one go.
    let backend = LiteRtLmBackend::new(options);
    backend
        .load(model_path)
        .with_context(|| format!("loading model {}", model_path.display()))
}

#[cfg(not(feature = "litertlm"))]
fn load_litertlm_backend(
    _session: &Session,
    _model_path: &Path,
) -> Result<Box<dyn aegis_inference_engine::LoadedModel>> {
    anyhow::bail!(
        "aegis run --backend litertlm requires the 'litertlm' Cargo feature; rebuild with \
         `cargo install --path crates/cli --features litertlm` (per ADR-023)"
    );
}

enum Halt {
    /// The mediator surfaced an expected-and-survivable error (Deny);
    /// the agent saw it can't, the ledger has the Violation entry, the
    /// run continues to the next script call.
    Continue,
    /// The mediator surfaced a halt-class error (rebind violation, or
    /// RequireApproval until F0-D wires real approval). The runtime
    /// must stop — break out of the script, but still shut the session
    /// down cleanly so the ledger root is captured.
    Stop(String),
    /// Unrecoverable plumbing failure (failed to read a Vec<u8>
    /// successfully after Allow, etc.) — propagate to the caller.
    Fatal(anyhow::Error),
}

fn dispatch(session: &mut Session, call: Call) -> std::result::Result<(), Halt> {
    let res = match call {
        Call::FilesystemRead {
            path,
            reasoning_step_id,
        } => session
            .mediate_filesystem_read(&path, reasoning_step_id.as_deref())
            .map(|_| ()),
        Call::FilesystemWrite {
            path,
            contents,
            reasoning_step_id,
        } => session.mediate_filesystem_write(
            &path,
            contents.as_bytes(),
            reasoning_step_id.as_deref(),
        ),
        Call::FilesystemDelete {
            path,
            reasoning_step_id,
        } => session.mediate_filesystem_delete(&path, reasoning_step_id.as_deref()),
        Call::NetworkOutbound {
            host,
            port,
            protocol,
            reasoning_step_id,
        } => {
            let proto = parse_proto(&protocol).map_err(Halt::Fatal)?;
            session
                .mediate_network_connect(&host, port, proto, reasoning_step_id.as_deref())
                .map(|_| ())
        }
        Call::Exec {
            program,
            args,
            reasoning_step_id,
        } => {
            let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            session
                .mediate_exec(&program, &args_ref, reasoning_step_id.as_deref())
                .map(|_| ())
        }
    };
    match res {
        Ok(()) => Ok(()),
        Err(RtError::Denied { .. }) => Err(Halt::Continue),
        Err(RtError::Policy(aegis_policy::Error::IdentityRebind(m))) => Err(Halt::Stop(format!(
            "identity rebind violation on {} digest",
            m.field
        ))),
        Err(RtError::RequireApproval { reason }) => Err(Halt::Stop(format!(
            "approval required (F0-D not wired in Phase 1a): {reason}"
        ))),
        Err(other) => Err(Halt::Fatal(anyhow::anyhow!("mediation failed: {other}"))),
    }
}

fn parse_proto(s: &str) -> Result<NetworkProto> {
    Ok(match s {
        "http" => NetworkProto::Http,
        "https" => NetworkProto::Https,
        "tcp" => NetworkProto::Tcp,
        "udp" => NetworkProto::Udp,
        "any" => NetworkProto::Any,
        other => anyhow::bail!("unknown protocol {other:?}"),
    })
}

fn load_script(path: &Path) -> Result<Script> {
    let bytes =
        std::fs::read(path).with_context(|| format!("reading script {}", path.display()))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing script {} as JSON", path.display()))
}

fn default_identity_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().context("could not resolve user config dir")?;
    Ok(base.join("aegis").join("identity"))
}

/// Tiny v7-shaped session-id generator without pulling the `uuid`
/// crate into `crates/cli` (it's already a transitive dep but adding
/// it directly is unnecessary churn). Format: hex of an `u128`
/// monotonically derived from `Instant::now`'s nanos and the process
/// id, dash-grouped to look UUID-shaped.
fn uuid_v7_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    let mixed = (nanos << 32) ^ pid;
    let hex = format!("{mixed:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..],
    )
}
