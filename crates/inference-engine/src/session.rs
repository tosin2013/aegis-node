//! Aegis-Node session lifecycle (F0-A — issue #24).
//!
//! `Session` is the runtime's top-level integration object. `boot` reads
//! a manifest + model + config, computes their SHA-256 digests, gets an
//! SVID with those digests bound in, opens the Trajectory Ledger, and
//! emits the `EntryType::SessionStart` entry. `shutdown` writes
//! `SessionEnd` and returns the chain root hash.
//!
//! The mediator (F0-B, #25) sits on top of `Session` and owns the
//! per-tool-call sequence: rebind → policy → gate → access entry. This
//! module deliberately does not implement that — boot is its own slice.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use aegis_approval_gate::ApprovalChannel;
use aegis_identity::{verify_digest_binding, Digest, DigestField, DigestTriple, LocalCa, SpiffeId};
use aegis_ledger_writer::{Entry, EntryType, LedgerWriter};
use aegis_policy::Policy;
use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};

use crate::error::{Error, Result};

/// Inputs to [`Session::boot`].
#[derive(Debug, Clone)]
pub struct BootConfig {
    /// Caller-supplied session identifier (UUIDv7 in production; tests
    /// pin it for golden output).
    pub session_id: String,
    pub manifest_path: PathBuf,
    pub model_path: PathBuf,
    /// Optional runtime config; absent → empty-bytes digest.
    pub config_path: Option<PathBuf>,
    pub identity_dir: PathBuf,
    pub workload_name: String,
    pub instance: String,
    pub ledger_path: PathBuf,
}

/// Live agent session: compiled policy, open ledger, issued SVID, the
/// digest triple bound at boot, and the agent identity hash that flows
/// into every ledger entry. Paths are retained so the F0-B mediator
/// can re-hash live bytes on every per-tool-call rebind check.
pub struct Session {
    policy: Policy,
    ledger: LedgerWriter,
    svid_cert_pem: String,
    svid_key_pem: String,
    bound_digests: DigestTriple,
    spiffe_id: SpiffeId,
    agent_identity_hash: [u8; 32],
    session_id: String,
    /// Wall-clock timestamp captured at boot. Used as the anchor for
    /// time-bounded write_grants (`duration: PT1H` means valid for the
    /// first hour of THIS session). Per ADR-009 / issue #38.
    pub(crate) session_start: DateTime<Utc>,
    pub(crate) manifest_path: PathBuf,
    pub(crate) model_path: PathBuf,
    pub(crate) config_path: Option<PathBuf>,
    /// F3 approval channel — routes `Decision::RequireApproval`. None
    /// means the legacy halt-on-RequireApproval behavior; set via
    /// [`Session::with_approval_channel`] after boot.
    pub(crate) approval_channel: Option<Box<dyn ApprovalChannel>>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // LedgerWriter holds a BufWriter<File> that isn't Debug; surface
        // only the operator-meaningful fields.
        f.debug_struct("Session")
            .field("session_id", &self.session_id)
            .field("spiffe_id", &self.spiffe_id)
            .field("bound_digests", &self.bound_digests)
            .finish_non_exhaustive()
    }
}

impl Session {
    /// Run the boot sequence end-to-end. On any failure the partial
    /// ledger is dropped (LedgerWriter cleans up via close-on-drop).
    pub fn boot(cfg: BootConfig) -> Result<Self> {
        let session_start = Utc::now();
        let policy = Policy::from_yaml_file(&cfg.manifest_path)?;

        let model_digest = sha256_file(&cfg.model_path)?;
        let manifest_digest = sha256_file(&cfg.manifest_path)?;
        let config_digest = match &cfg.config_path {
            Some(p) => sha256_file(p)?,
            None => sha256_bytes(&[]),
        };
        let bound_digests = DigestTriple {
            model: Digest(model_digest),
            manifest: Digest(manifest_digest),
            config: Digest(config_digest),
        };

        let ca = LocalCa::load(&cfg.identity_dir)?;
        let svid = ca.issue_svid(&cfg.workload_name, &cfg.instance, bound_digests)?;

        // Self-check: the cert we just got back MUST encode the digests
        // we passed in. If not, aegis-identity has a bug — fail loud.
        if let Some(mismatch) = verify_digest_binding(&svid.cert_pem, &bound_digests)? {
            return Err(Error::SvidSelfCheck {
                field: digest_field_name(mismatch.field).to_string(),
            });
        }

        let agent_identity_hash = sha256_bytes(svid.spiffe_id.uri().as_bytes());

        let mut ledger = LedgerWriter::create(&cfg.ledger_path, cfg.session_id.clone())?;

        let mut payload = Map::new();
        payload.insert("spiffeId".to_string(), Value::String(svid.spiffe_id.uri()));
        payload.insert(
            "modelDigestHex".to_string(),
            Value::String(hex::encode(bound_digests.model.0)),
        );
        payload.insert(
            "manifestDigestHex".to_string(),
            Value::String(hex::encode(bound_digests.manifest.0)),
        );
        payload.insert(
            "configDigestHex".to_string(),
            Value::String(hex::encode(bound_digests.config.0)),
        );
        ledger.append(Entry {
            session_id: cfg.session_id.clone(),
            entry_type: EntryType::SessionStart,
            agent_identity_hash,
            timestamp: Utc::now(),
            payload,
        })?;

        Ok(Self {
            policy,
            ledger,
            svid_cert_pem: svid.cert_pem,
            svid_key_pem: svid.key_pem,
            bound_digests,
            spiffe_id: svid.spiffe_id,
            agent_identity_hash,
            session_id: cfg.session_id,
            session_start,
            manifest_path: cfg.manifest_path,
            model_path: cfg.model_path,
            config_path: cfg.config_path,
            approval_channel: None,
        })
    }

    /// Attach an F3 approval channel. When set, `Decision::RequireApproval`
    /// is routed through `channel` (TTY prompt, file poll, etc.) before
    /// the mediator dispatches the operation. Without it, the mediator
    /// preserves the pre-#27 halt-on-RequireApproval behavior.
    pub fn with_approval_channel(mut self, channel: Box<dyn ApprovalChannel>) -> Self {
        self.approval_channel = Some(channel);
        self
    }

    /// Wall-clock anchor for time-bounded write_grants — set once at boot.
    pub fn session_start(&self) -> DateTime<Utc> {
        self.session_start
    }

    /// Emit `SessionEnd`, close the ledger, and return the chain root
    /// hash. The root is what an auditor pins to detect tampering.
    pub fn shutdown(mut self) -> Result<[u8; 32]> {
        let mut payload = Map::new();
        payload.insert("spiffeId".to_string(), Value::String(self.spiffe_id.uri()));
        self.ledger.append(Entry {
            session_id: self.session_id.clone(),
            entry_type: EntryType::SessionEnd,
            agent_identity_hash: self.agent_identity_hash,
            timestamp: Utc::now(),
            payload,
        })?;
        Ok(self.ledger.close()?)
    }

    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    pub fn spiffe_id(&self) -> &SpiffeId {
        &self.spiffe_id
    }

    pub fn agent_identity_hash(&self) -> [u8; 32] {
        self.agent_identity_hash
    }

    pub fn bound_digests(&self) -> &DigestTriple {
        &self.bound_digests
    }

    pub fn cert_pem(&self) -> &str {
        &self.svid_cert_pem
    }

    pub fn key_pem(&self) -> &str {
        &self.svid_key_pem
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Mutable access for the F0-B mediator and downstream emitters that
    /// need to append entries.
    pub fn ledger_writer_mut(&mut self) -> &mut LedgerWriter {
        &mut self.ledger
    }

    /// Re-hash the manifest + model + (optional) config files from disk
    /// and return the live digest triple. Used by the F0-B mediator's
    /// per-tool-call rebind step. Naive implementation re-reads on
    /// every call; Phase 2 will cache + invalidate via mtime.
    pub(crate) fn compute_live_digests(&self) -> Result<DigestTriple> {
        let model = sha256_file(&self.model_path)?;
        let manifest = sha256_file(&self.manifest_path)?;
        let config = match &self.config_path {
            Some(p) => sha256_file(p)?,
            None => sha256_bytes(&[]),
        };
        Ok(DigestTriple {
            model: Digest(model),
            manifest: Digest(manifest),
            config: Digest(config),
        })
    }
}

fn sha256_file(path: &Path) -> Result<[u8; 32]> {
    let f = File::open(path)?;
    let mut reader = BufReader::new(f);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&hasher.finalize());
    Ok(out)
}

fn sha256_bytes(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(&hasher.finalize());
    out
}

fn digest_field_name(f: DigestField) -> &'static str {
    match f {
        DigestField::Model => "model",
        DigestField::Manifest => "manifest",
        DigestField::Config => "config",
    }
}
