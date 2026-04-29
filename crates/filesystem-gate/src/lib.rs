//! Aegis-Node filesystem gate.
//!
//! Policy-checked thin wrappers around `std::fs`. Every read/write/delete
//! call goes through [`Policy::check_filesystem_*`] before the syscall;
//! denials are recorded in the Trajectory Ledger as `EntryType::Violation`
//! before the error returns, so the audit record exists even if the
//! runtime promptly halts.
//!
//! Closes the F2 acceptance criterion in #14: filesystem syscalls
//! (open/read/write/truncate/rename/unlink) must check the manifest
//! before proceeding.
//!
//! Bypass concerns: code that calls `std::fs` directly is out of this
//! gate's scope. Future hardening (seccomp, syscall auditing) is what
//! stops bypass attempts; this crate is the in-runtime first line.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use aegis_ledger_writer::LedgerWriter;
use aegis_policy::{emit_violation, Decision, Policy, ViolationEvent};
use chrono::{DateTime, Utc};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("policy: {0}")]
    Policy(#[from] aegis_policy::Error),

    #[error("filesystem policy denied: {reason}")]
    Denied { reason: String },

    #[error("filesystem policy requires approval: {reason}")]
    RequireApproval { reason: String },
}

pub type Result<T> = std::result::Result<T, Error>;

/// Bundles the things every filesystem operation needs: the compiled
/// policy, the ledger writer (for violation entries on Deny), the
/// agent identity hash, and the session start anchor for time-bounded
/// write_grants (per F7 / issue #38). Hold one per session; pass
/// `&mut self` into each call.
pub struct GateContext<'p, 'w> {
    policy: &'p Policy,
    writer: &'w mut LedgerWriter,
    agent_identity_hash: [u8; 32],
    session_start: DateTime<Utc>,
}

impl<'p, 'w> GateContext<'p, 'w> {
    pub fn new(
        policy: &'p Policy,
        writer: &'w mut LedgerWriter,
        agent_identity_hash: [u8; 32],
        session_start: DateTime<Utc>,
    ) -> Self {
        Self {
            policy,
            writer,
            agent_identity_hash,
            session_start,
        }
    }

    /// `std::fs::read` with policy enforcement.
    pub fn read(&mut self, path: impl AsRef<Path>) -> Result<Vec<u8>> {
        let p = path.as_ref();
        self.gate(p, AccessKind::Read)?;
        Ok(std::fs::read(p)?)
    }

    /// `std::fs::write` with policy enforcement (create + truncate).
    pub fn write(&mut self, path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
        let p = path.as_ref();
        self.gate(p, AccessKind::Write)?;
        std::fs::write(p, contents)?;
        Ok(())
    }

    /// Open for reading. Equivalent to `File::open` after a policy check.
    pub fn open_read(&mut self, path: impl AsRef<Path>) -> Result<File> {
        let p = path.as_ref();
        self.gate(p, AccessKind::Read)?;
        Ok(File::open(p)?)
    }

    /// Open for writing (create + truncate). Covers the F2 "truncate"
    /// criterion — there's no separate `truncate()` since `OpenOptions`
    /// with `truncate(true)` is the std way to truncate an existing file.
    pub fn open_write(&mut self, path: impl AsRef<Path>) -> Result<File> {
        let p = path.as_ref();
        self.gate(p, AccessKind::Write)?;
        Ok(OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(p)?)
    }

    /// `std::fs::remove_file` with policy enforcement.
    pub fn remove_file(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let p = path.as_ref();
        self.gate(p, AccessKind::Delete)?;
        std::fs::remove_file(p)?;
        Ok(())
    }

    /// `std::fs::remove_dir_all`. Top-level path is the policy check
    /// target; the recursive descent itself is not separately gated.
    pub fn remove_dir_all(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let p = path.as_ref();
        self.gate(p, AccessKind::Delete)?;
        std::fs::remove_dir_all(p)?;
        Ok(())
    }

    /// `std::fs::rename`. Both endpoints are gated: the source needs
    /// delete (it ceases to exist at that path) and the destination
    /// needs write. Either deny halts before any syscall runs.
    pub fn rename(&mut self, src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<()> {
        let s = src.as_ref();
        let d = dst.as_ref();
        self.gate(s, AccessKind::Delete)?;
        self.gate(d, AccessKind::Write)?;
        std::fs::rename(s, d)?;
        Ok(())
    }

    fn gate(&mut self, path: &Path, kind: AccessKind) -> Result<()> {
        let now = Utc::now();
        let decision = match kind {
            AccessKind::Read => self.policy.check_filesystem_read(path),
            AccessKind::Write => self
                .policy
                .check_filesystem_write(path, now, self.session_start),
            AccessKind::Delete => self
                .policy
                .check_filesystem_delete(path, now, self.session_start),
        };
        match decision {
            Decision::Allow => Ok(()),
            Decision::RequireApproval { reason } => Err(Error::RequireApproval { reason }),
            Decision::Deny { reason } => {
                let event = ViolationEvent {
                    reason: reason.clone(),
                    resource_uri: Some(format!("file://{}", absolutize(path).display())),
                    access_type: Some(kind.access_type_str().to_string()),
                    timestamp: Utc::now(),
                };
                emit_violation(self.writer, self.agent_identity_hash, event)?;
                Err(Error::Denied { reason })
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum AccessKind {
    Read,
    Write,
    Delete,
}

impl AccessKind {
    fn access_type_str(self) -> &'static str {
        match self {
            AccessKind::Read => "read",
            AccessKind::Write => "write",
            AccessKind::Delete => "delete",
        }
    }
}

/// Best-effort absolute path for the violation `resource_uri`. Falls back
/// to the input path if canonicalization isn't possible (the file may not
/// exist yet for create-paths, etc.).
fn absolutize(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .map(|cd| cd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}
