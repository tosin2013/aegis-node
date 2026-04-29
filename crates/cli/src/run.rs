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
use clap::Args;
use serde::Deserialize;

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

    /// JSON file with a fixed tool-call sequence.
    #[arg(long)]
    pub script: PathBuf,
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

    let script = load_script(&args.script)?;

    let cfg = BootConfig {
        session_id: session_id.clone(),
        manifest_path: args.manifest.clone(),
        model_path: args.model.clone(),
        config_path: args.config.clone(),
        identity_dir,
        workload_name: args.workload.clone(),
        instance: args.instance.clone(),
        ledger_path: ledger_path.clone(),
    };
    let mut session = Session::boot(cfg).context("boot")?;

    let mut halted = false;
    let mut halt_reason: Option<String> = None;
    for call in script.calls {
        match dispatch(&mut session, call) {
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
    let bytes = std::fs::read(path).with_context(|| format!("reading script {}", path.display()))?;
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
