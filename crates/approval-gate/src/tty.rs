//! TTY-channel approval — prompt on stdin/stderr with a timeout.
//!
//! The prompt is rendered to stderr (so it doesn't pollute stdout-
//! parsing callers) and reads a single line from stdin. To honor the
//! request timeout, the read happens on a worker thread that signals
//! via mpsc; the main thread does a `recv_timeout` select.

use std::io::{self, BufRead, Write};
use std::sync::mpsc;
use std::thread;

use chrono::Utc;

use crate::{ApprovalChannel, ApprovalOutcome, ApprovalRequest, Result};

/// Reads y/n on stdin. Default constructor uses real stdin/stderr;
/// the runtime's CLI wires this in. Tests prefer `FileApprovalChannel`
/// because driving an interactive stdin/stdout pair from a Rust unit
/// test is more pain than the coverage is worth.
pub struct TtyApprovalChannel;

impl TtyApprovalChannel {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TtyApprovalChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl ApprovalChannel for TtyApprovalChannel {
    fn request_approval(&mut self, req: &ApprovalRequest) -> Result<ApprovalOutcome> {
        let mut stderr = io::stderr().lock();
        writeln!(stderr, "── Aegis-Node approval requested ──")?;
        writeln!(stderr, "  session: {}", req.session_id)?;
        writeln!(stderr, "  action:  {}", req.action_summary)?;
        writeln!(stderr, "  target:  {}", req.resource_uri)?;
        writeln!(stderr, "  type:    {}", req.access_type)?;
        write!(
            stderr,
            "  approve? [y/N] (timeout {}s): ",
            req.timeout.as_secs()
        )?;
        stderr.flush()?;
        drop(stderr);

        let (tx, rx) = mpsc::channel::<io::Result<String>>();
        thread::spawn(move || {
            let mut line = String::new();
            let res = io::stdin().lock().read_line(&mut line).map(|_| line);
            let _ = tx.send(res);
        });

        match rx.recv_timeout(req.timeout) {
            Ok(Ok(line)) => {
                let answer = line.trim().to_lowercase();
                let now = Utc::now();
                if matches!(answer.as_str(), "y" | "yes") {
                    Ok(ApprovalOutcome::Granted {
                        approver_identity: "tty:local-operator".to_string(),
                        decided_at: now,
                    })
                } else {
                    Ok(ApprovalOutcome::Rejected {
                        reason: format!("operator declined ({answer:?})"),
                        decided_at: now,
                    })
                }
            }
            Ok(Err(e)) => Err(e.into()),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(ApprovalOutcome::TimedOut {
                expired_at: Utc::now(),
            }),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(crate::Error::Channel(
                "TTY reader thread disconnected without responding".to_string(),
            )),
        }
    }
}
