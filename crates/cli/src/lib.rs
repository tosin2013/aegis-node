//! `aegis` — Aegis-Node command-line interface (library half).
//!
//! Phase 1a ships:
//!
//! ```text
//! aegis identity init   --trust-domain <td>
//! aegis identity issue  <workload-name> --instance <i>
//!                       --model-digest <hex> --manifest-digest <hex>
//!                       --config-digest <hex>
//! aegis verify          <ledger-path> [--format text|json]
//! aegis run             --manifest <m> --model <m> --script <s> ...
//! ```
//!
//! The lib crate is the testable surface. `src/main.rs` is a thin
//! binary that calls `aegis_cli::run()`.

use std::path::PathBuf;

use aegis_identity::{Digest, DigestTriple, LocalCa};
use aegis_ledger_writer::{verify_file, VerifyError};
use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};

pub mod pull;
pub mod run;

#[derive(Debug, Parser)]
#[command(name = "aegis", version, about = "Aegis-Node CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Workload identity management (SPIFFE-compatible local CA, F1).
    Identity {
        #[command(subcommand)]
        sub: IdentityCommand,
    },
    /// Walk a Trajectory Ledger file and verify the SHA-256 hash chain (F9).
    Verify(VerifyArgs),
    /// Boot a session and run a fixed tool-call script (F0-E).
    Run(run::RunArgs),
    /// Fetch + verify a model artifact from an OCI registry (ADR-013, F1).
    Pull(PullArgs),
}

#[derive(Debug, Args)]
struct PullArgs {
    /// Reference to pull. Must include an `@sha256:<64 hex>` pin —
    /// tags alone are refused so the F1 SVID binding has a stable
    /// digest to commit to.
    reference: String,

    /// Override the default cache dir (default:
    /// `$XDG_CACHE_HOME/aegis/models` or platform equivalent).
    #[arg(long)]
    cache_dir: Option<PathBuf>,

    /// Path to a cosign public key (PEM). When omitted, keyless
    /// verification via Sigstore is used.
    #[arg(long)]
    cosign_key: Option<PathBuf>,

    /// Expected subject identity on the keyless certificate (regex).
    /// Strongly recommended in production; defaults to `.*`.
    #[arg(long)]
    keyless_identity: Option<String>,

    /// Expected OIDC issuer on the keyless certificate (regex).
    /// Strongly recommended in production; defaults to `.*`.
    #[arg(long)]
    keyless_oidc_issuer: Option<String>,
}

#[derive(Debug, Args)]
struct VerifyArgs {
    /// Path to the .jsonl ledger file.
    path: PathBuf,
    /// Output format. `json` is intended for CI/CD consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Subcommand)]
enum IdentityCommand {
    /// One-time setup of the local CA under `$XDG_CONFIG_HOME/aegis/identity`.
    Init(InitArgs),
    /// Issue a fresh X.509-SVID for `<workload-name>`.
    Issue(IssueArgs),
}

#[derive(Debug, Args)]
struct InitArgs {
    /// SPIFFE trust domain to embed in issued SVIDs (e.g. `aegis-node.local`).
    #[arg(long)]
    trust_domain: String,

    /// Override the default config dir.
    #[arg(long)]
    config_dir: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct IssueArgs {
    /// Workload name (path segment of the SPIFFE ID).
    workload_name: String,

    /// Instance identifier (last path segment of the SPIFFE ID).
    #[arg(long)]
    instance: String,

    /// SHA-256 digest of the model artifact, as a 64-char hex string.
    #[arg(long)]
    model_digest: String,

    /// SHA-256 digest of the resolved Permission Manifest.
    #[arg(long)]
    manifest_digest: String,

    /// SHA-256 digest of the runtime configuration.
    #[arg(long)]
    config_digest: String,

    /// Override the default config dir.
    #[arg(long)]
    config_dir: Option<PathBuf>,
}

/// Entry point for the binary. Parses argv via clap and dispatches.
pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Identity { sub } => match sub {
            IdentityCommand::Init(args) => cmd_init(args),
            IdentityCommand::Issue(args) => cmd_issue(args),
        },
        Command::Verify(args) => cmd_verify(args),
        Command::Run(args) => cmd_run(args),
        Command::Pull(args) => cmd_pull(args),
    }
}

fn cmd_init(args: InitArgs) -> Result<()> {
    let dir = resolve_identity_dir(args.config_dir)?;
    LocalCa::init(&dir, &args.trust_domain).with_context(|| {
        format!(
            "initializing local CA at {} for trust domain {:?}",
            dir.display(),
            args.trust_domain
        )
    })?;
    println!(
        "initialized Aegis-Node local CA at {} (trust_domain={})",
        dir.display(),
        args.trust_domain
    );
    Ok(())
}

fn cmd_issue(args: IssueArgs) -> Result<()> {
    let dir = resolve_identity_dir(args.config_dir)?;
    let ca =
        LocalCa::load(&dir).with_context(|| format!("loading local CA from {}", dir.display()))?;

    let digests = DigestTriple {
        model: parse_digest_arg("model-digest", &args.model_digest)?,
        manifest: parse_digest_arg("manifest-digest", &args.manifest_digest)?,
        config: parse_digest_arg("config-digest", &args.config_digest)?,
    };

    let svid = ca
        .issue_svid(&args.workload_name, &args.instance, digests)
        .with_context(|| format!("issuing SVID for {}/{}", args.workload_name, args.instance))?;

    println!("# spiffe_id: {}", svid.spiffe_id);
    println!("# model_digest: {}", svid.digests.model.hex());
    println!("# manifest_digest: {}", svid.digests.manifest.hex());
    println!("# config_digest: {}", svid.digests.config.hex());
    println!("{}", svid.cert_pem);
    println!("{}", svid.key_pem);
    Ok(())
}

fn resolve_identity_dir(override_dir: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = override_dir {
        return Ok(p);
    }
    let base = dirs::config_dir().context("could not resolve user config dir")?;
    Ok(base.join("aegis").join("identity"))
}

fn parse_digest_arg(name: &'static str, hex_str: &str) -> Result<Digest> {
    Digest::from_hex(hex_str).with_context(|| format!("--{name} must be a 64-char hex SHA-256"))
}

fn cmd_verify(args: VerifyArgs) -> Result<()> {
    match verify_file(&args.path) {
        Ok(summary) => {
            match args.format {
                OutputFormat::Text => {
                    let session = summary.session_id.as_deref().unwrap_or("(empty)");
                    let range = match (summary.first_timestamp, summary.last_timestamp) {
                        (Some(a), Some(b)) => format!("{a}..{b}"),
                        _ => "(no entries)".to_string(),
                    };
                    println!(
                        "ledger ok: session={session} entries={} root={} time={range}",
                        summary.entry_count, summary.root_hash_hex
                    );
                }
                OutputFormat::Json => {
                    let out = serde_json::json!({ "ok": true, "summary": summary });
                    println!("{}", serde_json::to_string(&out)?);
                }
            }
            Ok(())
        }
        Err(VerifyError::Break(brk)) => {
            match args.format {
                OutputFormat::Text => {
                    eprintln!("ledger break: {brk:?}");
                }
                OutputFormat::Json => {
                    let out = serde_json::json!({ "ok": false, "break": brk });
                    println!("{}", serde_json::to_string(&out)?);
                }
            }
            std::process::exit(1);
        }
        Err(VerifyError::Io(e)) => {
            Err(e).with_context(|| format!("opening ledger file {}", args.path.display()))
        }
    }
}

fn cmd_run(args: run::RunArgs) -> Result<()> {
    let outcome = run::execute(args)?;
    println!("ledger_root: {}", outcome.root_hash_hex);
    println!("ledger_path: {}", outcome.ledger_path.display());
    println!("entries: {}", outcome.entry_count);
    if outcome.halted {
        std::process::exit(1);
    }
    Ok(())
}

fn cmd_pull(args: PullArgs) -> Result<()> {
    let cache_dir = match args.cache_dir {
        Some(d) => d,
        None => pull::default_cache_dir().context("resolving default cache dir")?,
    };
    let cfg = pull::PullConfig {
        cache_dir,
        cosign_key: args.cosign_key,
        keyless_identity: args.keyless_identity,
        keyless_oidc_issuer: args.keyless_oidc_issuer,
    };
    let pulled = pull::pull(&args.reference, &cfg)
        .with_context(|| format!("pulling and verifying {}", args.reference))?;
    println!("# verified");
    println!("reference: {}", pulled.reference.canonical());
    println!("sha256: {}", pulled.sha256_hex);
    println!("blob_path: {}", pulled.blob_path.display());
    if let Some(template_sha) = &pulled.chat_template_sha256_hex {
        println!("chat_template_sha256: {template_sha}");
    }
    Ok(())
}
