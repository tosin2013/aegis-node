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

use aegis_identity::{verify_digest_binding, Digest, DigestField, DigestTriple, LocalCa, SpiffeId};
use aegis_ledger_writer::{Entry, EntryType, LedgerWriter};
use aegis_policy::Policy;
use chrono::Utc;
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
/// into every ledger entry.
pub struct Session {
    policy: Policy,
    ledger: LedgerWriter,
    svid_cert_pem: String,
    svid_key_pem: String,
    bound_digests: DigestTriple,
    spiffe_id: SpiffeId,
    agent_identity_hash: [u8; 32],
    session_id: String,
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
        })
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
