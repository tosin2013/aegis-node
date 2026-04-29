//! File-channel approval — polls a path for a JSON `{decision: ...}` blob.
//!
//! Used by the conformance harness and by automation that drives
//! approvals out-of-band (CI bots, scripted pipelines). The file's
//! presence triggers parsing; absence means "no approver action yet,
//! keep polling until timeout".

use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::Deserialize;

use crate::{ApprovalChannel, ApprovalOutcome, ApprovalRequest, Error, Result};

const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// JSON shape the file must contain. `approver` and `reason` are
/// optional and defaulted; only `decision` is required.
#[derive(Debug, Deserialize)]
struct FileResponse {
    decision: String,
    #[serde(default)]
    approver: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

/// Reads `path` for a `FileResponse` JSON blob. Constructed once per
/// session; mutates only its internal "first-poll" state.
pub struct FileApprovalChannel {
    path: PathBuf,
}

impl FileApprovalChannel {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }
}

impl ApprovalChannel for FileApprovalChannel {
    fn request_approval(&mut self, req: &ApprovalRequest) -> Result<ApprovalOutcome> {
        let deadline = Instant::now() + req.timeout;
        loop {
            if self.path.exists() {
                let bytes = std::fs::read(&self.path)?;
                let resp: FileResponse = serde_json::from_slice(&bytes)?;
                let now = Utc::now();
                return match resp.decision.as_str() {
                    "granted" => Ok(ApprovalOutcome::Granted {
                        approver_identity: resp
                            .approver
                            .unwrap_or_else(|| "file-channel".to_string()),
                        decided_at: now,
                    }),
                    "rejected" => Ok(ApprovalOutcome::Rejected {
                        reason: resp.reason.unwrap_or_else(|| "rejected".to_string()),
                        decided_at: now,
                    }),
                    other => Err(Error::Malformed(format!(
                        "decision must be \"granted\" or \"rejected\", got {other:?}"
                    ))),
                };
            }
            if Instant::now() >= deadline {
                return Ok(ApprovalOutcome::TimedOut {
                    expired_at: Utc::now(),
                });
            }
            thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())));
        }
    }
}
