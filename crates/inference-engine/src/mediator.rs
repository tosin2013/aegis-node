//! Per-tool-call mediation (F0-B — issue #25).
//!
//! Each `mediate_*` method on [`Session`] runs the same canonical
//! sequence:
//!
//! 1. **Rebind**: re-hash the boot inputs and call
//!    [`aegis_policy::check_identity_binding`]. Mismatch → Violation
//!    entry + `Error::Policy(IdentityRebind)`; the runtime should halt
//!    on this, but doing the actual halt is the caller's job.
//! 2. **Policy decision**: ask the engine.
//! 3. **Dispatch**:
//!    - `Allow` → run the syscall, then emit `EntryType::Access`.
//!    - `Deny` → emit `EntryType::Violation`, return `Error::Denied`.
//!    - `RequireApproval` → return `Error::RequireApproval` *without*
//!      emitting (approval is a flow, not a violation; F0-D / #27 wires
//!      the actual approval gate later).
//!
//! The mediator does its own emits rather than going through the
//! `network-gate` / `filesystem-gate` crates so we don't double-check
//! policy or double-emit violations. Those gates remain useful for
//! callers that aren't going through the runtime.

use std::fs;
use std::net::TcpStream;
use std::path::Path;
use std::process::{Command, Output};

use aegis_access_log::{
    emit_access, emit_reasoning_step, AccessEvent, AccessType, ReasoningStepEvent,
};
use aegis_policy::{check_identity_binding, Decision, NetworkProto};
use chrono::Utc;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::session::Session;

impl Session {
    /// Record a reasoning step (F5 per ADR-007) and return the step ID.
    /// The caller threads the returned ID into the next
    /// `mediate_*` call's `reasoning_step_id` parameter so an auditor
    /// can correlate the resulting Access entry back to the agent's
    /// stated rationale.
    ///
    /// Phase 1a accepts pre-computed reasoning text — the LLM-driven
    /// runtime that generates input/reasoning/tools_considered/
    /// tool_selected arrives in Phase 2 (ADR-014's llama.cpp Backend).
    pub fn record_reasoning_step(
        &mut self,
        input: impl Into<String>,
        reasoning: impl Into<String>,
        tools_considered: Vec<String>,
        tool_selected: Option<String>,
    ) -> Result<Uuid> {
        let event = ReasoningStepEvent {
            step_id: ReasoningStepEvent::new_v7_id(),
            input: input.into(),
            reasoning: reasoning.into(),
            tools_considered,
            tool_selected,
            timestamp: Utc::now(),
        };
        let step_id = event.step_id;
        let agent_hash = self.agent_identity_hash();
        emit_reasoning_step(self.ledger_writer_mut(), agent_hash, event)?;
        Ok(step_id)
    }

    /// Read a file with full mediation. Returns the file bytes.
    pub fn mediate_filesystem_read(
        &mut self,
        path: &Path,
        reasoning_step_id: Option<&str>,
    ) -> Result<Vec<u8>> {
        self.rebind()?;
        let decision = self.policy().check_filesystem_read(path);
        let resource_uri = file_uri(path);
        match decision {
            Decision::Allow => {
                let bytes = fs::read(path)?;
                self.emit_success(
                    &resource_uri,
                    AccessType::Read,
                    bytes.len() as u64,
                    reasoning_step_id,
                )?;
                Ok(bytes)
            }
            Decision::Deny { reason } => {
                self.emit_deny(&resource_uri, "read", &reason)?;
                Err(Error::Denied { reason })
            }
            Decision::RequireApproval { reason } => Err(Error::RequireApproval { reason }),
        }
    }

    /// Write a file with full mediation. Truncates and creates as needed.
    pub fn mediate_filesystem_write(
        &mut self,
        path: &Path,
        contents: &[u8],
        reasoning_step_id: Option<&str>,
    ) -> Result<()> {
        self.rebind()?;
        let now = Utc::now();
        let session_start = self.session_start();
        let decision = self
            .policy()
            .check_filesystem_write(path, now, session_start);
        let resource_uri = file_uri(path);
        match decision {
            Decision::Allow => {
                fs::write(path, contents)?;
                self.emit_success(
                    &resource_uri,
                    AccessType::Write,
                    contents.len() as u64,
                    reasoning_step_id,
                )?;
                Ok(())
            }
            Decision::Deny { reason } => {
                self.emit_deny(&resource_uri, "write", &reason)?;
                Err(Error::Denied { reason })
            }
            Decision::RequireApproval { reason } => Err(Error::RequireApproval { reason }),
        }
    }

    /// Delete a file with full mediation.
    pub fn mediate_filesystem_delete(
        &mut self,
        path: &Path,
        reasoning_step_id: Option<&str>,
    ) -> Result<()> {
        self.rebind()?;
        let now = Utc::now();
        let session_start = self.session_start();
        let decision = self
            .policy()
            .check_filesystem_delete(path, now, session_start);
        let resource_uri = file_uri(path);
        match decision {
            Decision::Allow => {
                fs::remove_file(path)?;
                self.emit_success(&resource_uri, AccessType::Delete, 0, reasoning_step_id)?;
                Ok(())
            }
            Decision::Deny { reason } => {
                self.emit_deny(&resource_uri, "delete", &reason)?;
                Err(Error::Denied { reason })
            }
            Decision::RequireApproval { reason } => Err(Error::RequireApproval { reason }),
        }
    }

    /// Outbound TCP connect with full mediation. Returns the open stream.
    pub fn mediate_network_connect(
        &mut self,
        host: &str,
        port: u16,
        proto: NetworkProto,
        reasoning_step_id: Option<&str>,
    ) -> Result<TcpStream> {
        self.rebind()?;
        let decision = self.policy().check_network_outbound(host, port, proto);
        let resource_uri = network_uri(host, port, proto);
        match decision {
            Decision::Allow => {
                let stream = TcpStream::connect((host, port))?;
                self.emit_success(
                    &resource_uri,
                    AccessType::NetworkOutbound,
                    0,
                    reasoning_step_id,
                )?;
                Ok(stream)
            }
            Decision::Deny { reason } => {
                self.emit_deny(&resource_uri, "network_outbound", &reason)?;
                Err(Error::Denied { reason })
            }
            Decision::RequireApproval { reason } => Err(Error::RequireApproval { reason }),
        }
    }

    /// Exec a program with full mediation. Returns the captured Output.
    pub fn mediate_exec(
        &mut self,
        program: &Path,
        args: &[&str],
        reasoning_step_id: Option<&str>,
    ) -> Result<Output> {
        self.rebind()?;
        let decision = self.policy().check_exec(program);
        let resource_uri = format!("exec://{}", program.display());
        match decision {
            Decision::Allow => {
                let output = Command::new(program).args(args).output()?;
                self.emit_success(&resource_uri, AccessType::Exec, 0, reasoning_step_id)?;
                Ok(output)
            }
            Decision::Deny { reason } => {
                self.emit_deny(&resource_uri, "exec", &reason)?;
                Err(Error::Denied { reason })
            }
            Decision::RequireApproval { reason } => Err(Error::RequireApproval { reason }),
        }
    }

    fn rebind(&mut self) -> Result<()> {
        let live = self.compute_live_digests()?;
        let cert_pem = self.cert_pem().to_string();
        let agent_hash = self.agent_identity_hash();
        check_identity_binding(
            self.ledger_writer_mut(),
            agent_hash,
            &cert_pem,
            &live,
            Utc::now(),
        )?;
        Ok(())
    }

    fn emit_success(
        &mut self,
        resource_uri: &str,
        access_type: AccessType,
        bytes: u64,
        reasoning_step_id: Option<&str>,
    ) -> Result<()> {
        let agent_hash = self.agent_identity_hash();
        emit_access(
            self.ledger_writer_mut(),
            agent_hash,
            AccessEvent {
                resource_uri: resource_uri.to_string(),
                access_type,
                bytes_accessed: bytes,
                reasoning_step_id: reasoning_step_id.map(str::to_string),
                timestamp: Utc::now(),
            },
        )?;
        Ok(())
    }

    fn emit_deny(&mut self, resource_uri: &str, access_kind: &str, reason: &str) -> Result<()> {
        let agent_hash = self.agent_identity_hash();
        aegis_policy::emit_violation(
            self.ledger_writer_mut(),
            agent_hash,
            aegis_policy::ViolationEvent {
                reason: reason.to_string(),
                resource_uri: Some(resource_uri.to_string()),
                access_type: Some(access_kind.to_string()),
                timestamp: Utc::now(),
            },
        )?;
        Ok(())
    }
}

fn file_uri(path: &Path) -> String {
    if path.is_absolute() {
        format!("file://{}", path.display())
    } else {
        // Best-effort canonicalize; if cwd lookup fails, fall back to
        // the raw path so audit still has *something* to correlate.
        match std::env::current_dir() {
            Ok(cwd) => format!("file://{}", cwd.join(path).display()),
            Err(_) => format!("file://{}", path.display()),
        }
    }
}

fn network_uri(host: &str, port: u16, proto: NetworkProto) -> String {
    let scheme = match proto {
        NetworkProto::Http => "http",
        NetworkProto::Https => "https",
        NetworkProto::Udp => "udp",
        NetworkProto::Tcp | NetworkProto::Any => "tcp",
    };
    format!("{scheme}://{host}:{port}")
}
