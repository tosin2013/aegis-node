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
use aegis_approval_gate::{ApprovalOutcome, ApprovalRequest, DEFAULT_TIMEOUT};
use aegis_ledger_writer::{Entry, EntryType};
use aegis_policy::{check_identity_binding, Decision, NetworkProto};
use chrono::Utc;
use serde_json::{Map, Value};
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
        let decision =
            self.route_through_approval(decision, &resource_uri, "read", reasoning_step_id)?;
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
        let decision =
            self.route_through_approval(decision, &resource_uri, "write", reasoning_step_id)?;
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
        let decision =
            self.route_through_approval(decision, &resource_uri, "delete", reasoning_step_id)?;
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
    /// Every call appends one entry to the F6 attestation accumulator
    /// (issue #37) so `shutdown` can summarize the session's network
    /// activity, regardless of whether the connection was allowed,
    /// denied, or routed through approval.
    pub fn mediate_network_connect(
        &mut self,
        host: &str,
        port: u16,
        proto: NetworkProto,
        reasoning_step_id: Option<&str>,
    ) -> Result<TcpStream> {
        self.rebind()?;
        let initial = self.policy().check_network_outbound(host, port, proto);
        let was_approval_required = matches!(initial, Decision::RequireApproval { .. });
        let resource_uri = network_uri(host, port, proto);
        let routed = self.route_through_approval(
            initial,
            &resource_uri,
            "network_outbound",
            reasoning_step_id,
        );
        let proto_str = match proto {
            NetworkProto::Http => "http",
            NetworkProto::Https => "https",
            NetworkProto::Udp => "udp",
            NetworkProto::Tcp | NetworkProto::Any => "tcp",
        };

        let routed = match routed {
            Ok(d) => d,
            Err(e) => {
                // Approval rejected/timed out — record as denied.
                self.network_log
                    .push(crate::session::NetworkConnectionMeta {
                        host: host.to_string(),
                        port,
                        protocol: proto_str.to_string(),
                        decision: crate::session::NetworkConnectionDecision::Denied,
                        timestamp: Utc::now(),
                    });
                return Err(e);
            }
        };

        match routed {
            Decision::Allow => {
                let stream = TcpStream::connect((host, port))?;
                self.emit_success(
                    &resource_uri,
                    AccessType::NetworkOutbound,
                    0,
                    reasoning_step_id,
                )?;
                let kind = if was_approval_required {
                    crate::session::NetworkConnectionDecision::Approved
                } else {
                    crate::session::NetworkConnectionDecision::Allowed
                };
                self.network_log
                    .push(crate::session::NetworkConnectionMeta {
                        host: host.to_string(),
                        port,
                        protocol: proto_str.to_string(),
                        decision: kind,
                        timestamp: Utc::now(),
                    });
                Ok(stream)
            }
            Decision::Deny { reason } => {
                self.emit_deny(&resource_uri, "network_outbound", &reason)?;
                self.network_log
                    .push(crate::session::NetworkConnectionMeta {
                        host: host.to_string(),
                        port,
                        protocol: proto_str.to_string(),
                        decision: crate::session::NetworkConnectionDecision::Denied,
                        timestamp: Utc::now(),
                    });
                Err(Error::Denied { reason })
            }
            Decision::RequireApproval { reason } => {
                // Legacy halt path — channel not configured.
                self.network_log
                    .push(crate::session::NetworkConnectionMeta {
                        host: host.to_string(),
                        port,
                        protocol: proto_str.to_string(),
                        decision: crate::session::NetworkConnectionDecision::Denied,
                        timestamp: Utc::now(),
                    });
                Err(Error::RequireApproval { reason })
            }
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
        let decision =
            self.route_through_approval(decision, &resource_uri, "exec", reasoning_step_id)?;
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

    /// Route a `Decision::RequireApproval` through the configured F3
    /// channel (TTY / file / future web UI). Emits the
    /// ApprovalRequest → Granted/Rejected/TimedOut entry pair, returns:
    ///
    /// - `Ok(Decision::Allow)` if granted (caller proceeds; will emit Access).
    /// - `Err(Error::Denied)` if rejected or timed out — already-emitted
    ///   ApprovalRejected/ApprovalTimedOut entry takes the place of a
    ///   Violation, since approval-rejection is a legitimate flow per
    ///   ADR-005, not a security violation.
    /// - `Ok(decision)` unchanged when the input isn't `RequireApproval`
    ///   or when no channel is configured (legacy halt-on-RequireApproval
    ///   behavior preserved for callers that opt out).
    fn route_through_approval(
        &mut self,
        decision: Decision,
        resource_uri: &str,
        access_kind: &str,
        reasoning_step_id: Option<&str>,
    ) -> Result<Decision> {
        let summary = match &decision {
            Decision::RequireApproval { reason } => reason.clone(),
            _ => return Ok(decision),
        };
        if self.approval_channel.is_none() {
            return Ok(decision);
        }

        let req = ApprovalRequest {
            action_summary: summary,
            resource_uri: resource_uri.to_string(),
            access_type: access_kind.to_string(),
            session_id: self.session_id().to_string(),
            reasoning_step_id: reasoning_step_id.map(str::to_string),
            timeout: DEFAULT_TIMEOUT,
        };
        self.emit_approval_request(&req)?;

        // Take the channel to release the &mut self borrow on
        // approval_channel for the duration of the call; put it back
        // after (the channel is reusable across requests). The is_none
        // check above guarantees this branch matches; using a match
        // instead of expect() keeps clippy::expect_used happy.
        let mut channel = match self.approval_channel.take() {
            Some(c) => c,
            None => return Ok(decision),
        };
        let outcome = channel.request_approval(&req);
        self.approval_channel = Some(channel);
        let outcome = outcome.map_err(|e| Error::Denied {
            reason: format!("approval channel: {e}"),
        })?;

        match outcome {
            ApprovalOutcome::Granted {
                approver_identity,
                decided_at,
            } => {
                self.emit_approval_granted(&req, &approver_identity, decided_at)?;
                Ok(Decision::Allow)
            }
            ApprovalOutcome::Rejected { reason, decided_at } => {
                self.emit_approval_rejected(&req, &reason, decided_at)?;
                Err(Error::Denied {
                    reason: format!("approval rejected: {reason}"),
                })
            }
            ApprovalOutcome::TimedOut { expired_at } => {
                self.emit_approval_timed_out(&req, expired_at)?;
                Err(Error::Denied {
                    reason: "approval timed out".to_string(),
                })
            }
        }
    }

    fn emit_approval_request(&mut self, req: &ApprovalRequest) -> Result<()> {
        let agent_hash = self.agent_identity_hash();
        let session_id = self.session_id().to_string();
        let mut payload = Map::new();
        payload.insert(
            "actionSummary".to_string(),
            Value::String(req.action_summary.clone()),
        );
        payload.insert(
            "resourceUri".to_string(),
            Value::String(req.resource_uri.clone()),
        );
        payload.insert(
            "accessType".to_string(),
            Value::String(req.access_type.clone()),
        );
        if let Some(rsid) = &req.reasoning_step_id {
            payload.insert("reasoningStepId".to_string(), Value::String(rsid.clone()));
        }
        payload.insert(
            "expiresAt".to_string(),
            Value::String(
                (Utc::now() + chrono::Duration::from_std(req.timeout).unwrap_or_default())
                    .to_rfc3339(),
            ),
        );
        self.ledger_writer_mut().append(Entry {
            session_id,
            entry_type: EntryType::ApprovalRequest,
            agent_identity_hash: agent_hash,
            timestamp: Utc::now(),
            payload,
        })?;
        Ok(())
    }

    fn emit_approval_granted(
        &mut self,
        req: &ApprovalRequest,
        approver_identity: &str,
        decided_at: chrono::DateTime<Utc>,
    ) -> Result<()> {
        let agent_hash = self.agent_identity_hash();
        let session_id = self.session_id().to_string();
        let mut payload = Map::new();
        payload.insert(
            "approverId".to_string(),
            Value::String(approver_identity.to_string()),
        );
        payload.insert("decision".to_string(), Value::String("granted".to_string()));
        payload.insert(
            "decidedAt".to_string(),
            Value::String(decided_at.to_rfc3339()),
        );
        if let Some(rsid) = &req.reasoning_step_id {
            payload.insert("reasoningStepId".to_string(), Value::String(rsid.clone()));
        }
        self.ledger_writer_mut().append(Entry {
            session_id,
            entry_type: EntryType::ApprovalGranted,
            agent_identity_hash: agent_hash,
            timestamp: Utc::now(),
            payload,
        })?;
        Ok(())
    }

    fn emit_approval_rejected(
        &mut self,
        req: &ApprovalRequest,
        reason: &str,
        decided_at: chrono::DateTime<Utc>,
    ) -> Result<()> {
        let agent_hash = self.agent_identity_hash();
        let session_id = self.session_id().to_string();
        let mut payload = Map::new();
        payload.insert(
            "decision".to_string(),
            Value::String("rejected".to_string()),
        );
        payload.insert(
            "decidedAt".to_string(),
            Value::String(decided_at.to_rfc3339()),
        );
        payload.insert(
            "violationReason".to_string(),
            Value::String(reason.to_string()),
        );
        if let Some(rsid) = &req.reasoning_step_id {
            payload.insert("reasoningStepId".to_string(), Value::String(rsid.clone()));
        }
        self.ledger_writer_mut().append(Entry {
            session_id,
            entry_type: EntryType::ApprovalRejected,
            agent_identity_hash: agent_hash,
            timestamp: Utc::now(),
            payload,
        })?;
        Ok(())
    }

    fn emit_approval_timed_out(
        &mut self,
        req: &ApprovalRequest,
        expired_at: chrono::DateTime<Utc>,
    ) -> Result<()> {
        let agent_hash = self.agent_identity_hash();
        let session_id = self.session_id().to_string();
        let mut payload = Map::new();
        payload.insert(
            "decision".to_string(),
            Value::String("timed_out".to_string()),
        );
        payload.insert(
            "decidedAt".to_string(),
            Value::String(expired_at.to_rfc3339()),
        );
        if let Some(rsid) = &req.reasoning_step_id {
            payload.insert("reasoningStepId".to_string(), Value::String(rsid.clone()));
        }
        self.ledger_writer_mut().append(Entry {
            session_id,
            entry_type: EntryType::ApprovalTimedOut,
            agent_identity_hash: agent_hash,
            timestamp: Utc::now(),
            payload,
        })?;
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
