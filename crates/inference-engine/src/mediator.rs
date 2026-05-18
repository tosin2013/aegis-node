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

use std::time::{Duration, SystemTime};

use aegis_access_log::{
    emit_access, emit_reasoning_step, AccessEvent, AccessType, ReasoningStepEvent,
};
use aegis_approval_gate::{
    ApprovalGrant, ApprovalOutcome, ApprovalRequest, GrantDecision, DEFAULT_TIMEOUT,
};
use aegis_ledger_writer::{Entry, EntryType};
use aegis_policy::{
    check_identity_binding,
    manifest::{ApprovalPolicy, ApprovalTier, PreValidateClause, PreValidateKind},
    Decision, NetworkProto, ToolClass,
};
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
        let canonical_args = serde_json::json!({"path": path.display().to_string()});
        let decision = self.route_through_approval(
            decision,
            &resource_uri,
            "read",
            reasoning_step_id,
            ToolClass::Filesystem,
            "filesystem__read",
            &canonical_args,
        )?;
        match decision {
            Decision::Allow => {
                self.enforce_aggregate_quota(ToolClass::Filesystem, &resource_uri, "read")?;
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
        // Hash contents into the canonical arg signature so write
        // grants don't auto-consume across changed bodies — same path
        // + same bytes is the same operation; same path + new bytes
        // is a fresh prompt.
        let contents_hex = hex::encode({
            use sha2::{Digest as _, Sha256};
            let mut h = Sha256::new();
            h.update(contents);
            h.finalize()
        });
        let canonical_args = serde_json::json!({
            "path": path.display().to_string(),
            "contentsSha256": contents_hex,
        });
        let decision = self.route_through_approval(
            decision,
            &resource_uri,
            "write",
            reasoning_step_id,
            ToolClass::Filesystem,
            "filesystem__write",
            &canonical_args,
        )?;
        match decision {
            Decision::Allow => {
                self.enforce_aggregate_quota(ToolClass::Filesystem, &resource_uri, "write")?;
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
        let canonical_args = serde_json::json!({"path": path.display().to_string()});
        let decision = self.route_through_approval(
            decision,
            &resource_uri,
            "delete",
            reasoning_step_id,
            ToolClass::Filesystem,
            "filesystem__delete",
            &canonical_args,
        )?;
        match decision {
            Decision::Allow => {
                self.enforce_aggregate_quota(ToolClass::Filesystem, &resource_uri, "delete")?;
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
        let proto_str = match proto {
            NetworkProto::Http => "http",
            NetworkProto::Https => "https",
            NetworkProto::Udp => "udp",
            NetworkProto::Tcp | NetworkProto::Any => "tcp",
        };
        let canonical_args = serde_json::json!({
            "host": host,
            "port": port,
            "protocol": proto_str,
        });
        let routed = self.route_through_approval(
            initial,
            &resource_uri,
            "network_outbound",
            reasoning_step_id,
            ToolClass::Network,
            "network__connect",
            &canonical_args,
        );

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

    /// MCP tool call with full mediation (per ADR-018 / F2-MCP-B / issue
    /// #44). Resolves `(server_name, tool_name)` against the manifest's
    /// `tools.mcp[]` allowlist; on `Allow`, dispatches via the configured
    /// MCP client and emits one `EntryType::Access` with
    /// `accessType=mcp_tool_call`. On `Deny`, emits a Violation and
    /// returns `Error::Denied`. On a missing MCP client, treats the call
    /// as denied with reason `"no mcp client configured"` so audit still
    /// shows the attempt.
    ///
    /// Side-effects produced *inside* the upstream tool (filesystem
    /// reads, network connects, ...) emit their own access entries
    /// through the existing `mediate_*` methods — this entry is the
    /// summary, not a replacement.
    pub fn mediate_mcp_tool_call(
        &mut self,
        server_name: &str,
        tool_name: &str,
        args: serde_json::Value,
        reasoning_step_id: Option<&str>,
    ) -> Result<serde_json::Value> {
        self.rebind()?;
        let decision = self.policy().check_mcp_tool(server_name, tool_name);
        let resource_uri = mcp_uri(server_name, tool_name);
        match decision {
            Decision::Allow => {}
            Decision::Deny { reason } => {
                self.emit_deny(&resource_uri, "mcp_tool_call", &reason)?;
                return Err(Error::Denied { reason });
            }
            Decision::RequireApproval { reason } => {
                // Phase 1: approval gates over MCP not yet wired.
                return Err(Error::RequireApproval { reason });
            }
        }
        let server_grant = match self
            .policy()
            .manifest()
            .tools
            .mcp
            .iter()
            .find(|s| s.server_name == server_name)
        {
            Some(s) => s,
            None => {
                let reason = format!(
                    "mcp tool call to server {server_name:?} not granted: server not in tools.mcp[]",
                );
                self.emit_deny(&resource_uri, "mcp_tool_call", &reason)?;
                return Err(Error::Denied { reason });
            }
        };
        let server_uri = server_grant.server_uri.clone();

        // ADR-024-B: per-tool pre-validation. After the MCP allowlist
        // check passes (above), look up this tool's `allowed_tools`
        // entry. If it's an object form with `pre_validate` clauses,
        // run each clause against the matching `policy.check_*` gate
        // before dispatching to the MCP server. The shorthand-string
        // form has no clauses → behavior is unchanged (one-layer
        // enforcement). Denials surface as Violation entries with a
        // distinguishable `mcp-prevalidate://server/tool?arg=value`
        // resource_uri so an auditor can tell which layer refused.
        let pre_validate_clauses: Vec<PreValidateClause> = server_grant
            .allowed_tools
            .iter()
            .find(|t| t.name() == tool_name)
            .map(|t| t.pre_validate().to_vec())
            .unwrap_or_default();
        if !pre_validate_clauses.is_empty() {
            self.run_mcp_pre_validate(
                server_name,
                tool_name,
                &args,
                &pre_validate_clauses,
                reasoning_step_id,
            )?;
        }

        // ADR-027 aggregate quota — refuse the call if the per-server
        // counter would breach `tools.mcp[].quota.max_calls_per_session`.
        self.enforce_aggregate_quota(
            ToolClass::Mcp(server_name.to_string()),
            &resource_uri,
            "mcp_tool_call",
        )?;

        let mut client = match self.mcp_client.take() {
            Some(c) => c,
            None => {
                let reason = "no mcp client configured for session".to_string();
                self.emit_deny(&resource_uri, "mcp_tool_call", &reason)?;
                return Err(Error::Denied { reason });
            }
        };
        let result = client.call_tool(&server_uri, tool_name, args);
        self.mcp_client = Some(client);
        let value = result.map_err(|e| Error::Denied {
            reason: format!("mcp client: {e}"),
        })?;
        let bytes = serde_json::to_vec(&value)
            .map(|v| v.len() as u64)
            .unwrap_or(0);
        self.emit_success(
            &resource_uri,
            AccessType::McpToolCall,
            bytes,
            reasoning_step_id,
        )?;
        Ok(value)
    }

    /// ADR-024-B: run the manifest-declared `pre_validate` clauses
    /// for an MCP tool call between the allowlist check and the
    /// dispatch to the MCP server.
    ///
    /// On the first denial: emit a Violation entry with a
    /// `mcp-prevalidate://<server>/<tool>?<arg>=<value>` resource_uri
    /// (so an auditor can distinguish "MCP allowlist denied" from
    /// "filesystem gate denied via MCP pre-validation"), then return
    /// [`Error::Denied`]. Subsequent clauses on the same call do not
    /// run — the call is refused as a whole.
    ///
    /// On a malformed arg (clause names a field that's missing or
    /// the wrong shape), return [`Error::McpPreValidateMalformedArg`].
    /// The model's tool call gets denied — better than dispatching
    /// against an undefined arg or crashing the session.
    fn run_mcp_pre_validate(
        &mut self,
        server_name: &str,
        tool_name: &str,
        args: &Value,
        clauses: &[PreValidateClause],
        _reasoning_step_id: Option<&str>,
    ) -> Result<()> {
        for clause in clauses {
            let values = extract_clause_values(args, server_name, tool_name, clause)?;
            // Capture session_start before any &mut self call below
            // so we don't conflict with the immutable policy() borrow.
            let session_start = self.session_start();
            for value in values {
                let arg_name = clause
                    .arg
                    .as_deref()
                    .or(clause.arg_array.as_deref())
                    .unwrap_or("?");
                let decision = match clause.kind {
                    PreValidateKind::FilesystemRead => {
                        self.policy().check_filesystem_read(Path::new(&value))
                    }
                    PreValidateKind::FilesystemWrite => {
                        let now = Utc::now();
                        self.policy()
                            .check_filesystem_write(Path::new(&value), now, session_start)
                    }
                    PreValidateKind::FilesystemDelete => {
                        let now = Utc::now();
                        self.policy()
                            .check_filesystem_delete(Path::new(&value), now, session_start)
                    }
                    PreValidateKind::NetworkOutbound => {
                        let (host, port) =
                            parse_pre_validate_url(&value, server_name, tool_name, arg_name)?;
                        self.policy()
                            .check_network_outbound(&host, port, NetworkProto::Tcp)
                    }
                };
                match decision {
                    Decision::Allow => continue,
                    Decision::Deny { reason } => {
                        let resource_uri = format!(
                            "mcp-prevalidate://{server_name}/{tool_name}?{arg_name}={value}"
                        );
                        self.emit_deny(&resource_uri, "mcp_pre_validate", &reason)?;
                        return Err(Error::Denied { reason });
                    }
                    Decision::RequireApproval { reason } => {
                        // Phase 1 — approvals over MCP pre-validate
                        // not yet wired (parity with the existing
                        // mediate_mcp_tool_call branch).
                        return Err(Error::RequireApproval { reason });
                    }
                }
            }
        }
        Ok(())
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
        let canonical_args = serde_json::json!({
            "program": program.display().to_string(),
            "args": args,
        });
        let decision = self.route_through_approval(
            decision,
            &resource_uri,
            "exec",
            reasoning_step_id,
            ToolClass::Exec,
            "exec__run",
            &canonical_args,
        )?;
        match decision {
            Decision::Allow => {
                self.enforce_aggregate_quota(ToolClass::Exec, &resource_uri, "exec")?;
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
    /// channel (TTY / file / future web UI). Per ADR-029, the gate
    /// now also consults the task-scoped grant table — identical
    /// retries of `(tool_name, sha256(canonical_args))` within the
    /// manifest-configured TTL auto-consume the prior decision
    /// without re-prompting. Tier behavior (advisory/validating) is
    /// applied per the manifest's `tools.<class>.approval` block;
    /// blocking and escalating fall through to validating behavior
    /// in the foundation PR.
    ///
    /// Returns:
    /// - `Ok(Decision::Allow)` if granted (caller proceeds; will emit Access).
    /// - `Err(Error::Denied)` if rejected or timed out — already-emitted
    ///   ApprovalRejected/ApprovalTimedOut entry takes the place of a
    ///   Violation, since approval-rejection is a legitimate flow per
    ///   ADR-005, not a security violation.
    /// - `Ok(decision)` unchanged when the input isn't `RequireApproval`
    ///   or when no channel is configured (legacy halt-on-RequireApproval
    ///   behavior preserved for callers that opt out).
    #[allow(clippy::too_many_arguments)]
    fn route_through_approval(
        &mut self,
        decision: Decision,
        resource_uri: &str,
        access_kind: &str,
        reasoning_step_id: Option<&str>,
        tool_class: ToolClass,
        tool_name: &str,
        canonical_args: &Value,
    ) -> Result<Decision> {
        let summary = match &decision {
            Decision::RequireApproval { reason } => reason.clone(),
            _ => return Ok(decision),
        };

        // ADR-029 §"Risk-tiered approval scopes" — pull the per-class
        // tier and TTL once. Default to the legacy halt-and-prompt
        // (Validating, 5-minute TTL) when the manifest doesn't declare
        // an `approval` block.
        let policy = self.approval_policy_for(&tool_class);
        let tier = policy.map(|p| p.tier).unwrap_or_default();
        let ttl = Duration::from_secs(
            policy
                .map(|p| p.grant_ttl_seconds)
                .unwrap_or(DEFAULT_GRANT_TTL_SECS),
        );

        // Advisory: ADR-029 §"Risk-tiered approval scopes" —
        // "Log to ledger, dispatch immediately. No prompt." Emit the
        // approval_decision entry as audit then return Allow.
        if matches!(tier, ApprovalTier::Advisory) {
            self.emit_approval_decision_advisory(
                tool_name,
                canonical_args,
                resource_uri,
                access_kind,
                reasoning_step_id,
            )?;
            return Ok(Decision::Allow);
        }

        // Foundation slice: Blocking and Escalating fall through to
        // the Validating prompt flow. Behavior switching for them is
        // tracked as a deferred follow-up — see ADR-029 §"Risk-tiered
        // approval scopes" + PR #198 body.
        let _ = ApprovalTier::Blocking;
        let _ = ApprovalTier::Escalating;

        // Consult the grant table BEFORE prompting. Live grant for
        // identical (tool_name, canonical_args) within TTL: emit
        // approval_decision (auto-consumed) and reuse the cached
        // decision — Allow dispatches, Deny short-circuits with the
        // original reason. We clone the few fields we need so the
        // immutable lookup-borrow doesn't conflict with the mutable
        // emit_* call below.
        let maybe_grant = self
            .grant_table
            .lookup(tool_name, canonical_args, SystemTime::now())
            .map(|g| (g.arg_hash_hex(), g.grant_id.to_string(), g.decision.clone()));
        if let Some((arg_hash_hex, grant_id, cached)) = maybe_grant {
            self.emit_approval_decision_auto_consumed(
                tool_name,
                resource_uri,
                access_kind,
                reasoning_step_id,
                &grant_id,
                &arg_hash_hex,
                &cached,
            )?;
            return match cached {
                GrantDecision::Allow => Ok(Decision::Allow),
                GrantDecision::Deny { reason } => Err(Error::Denied {
                    reason: format!("approval grant denied: {reason}"),
                }),
            };
        }

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
                let grant = ApprovalGrant::allow(tool_name, canonical_args, ttl);
                let grant_id = grant.grant_id.to_string();
                let arg_hash_hex = grant.arg_hash_hex();
                self.grant_table.insert(grant);
                self.emit_approval_decision_granted(
                    tool_name,
                    resource_uri,
                    access_kind,
                    reasoning_step_id,
                    &grant_id,
                    &arg_hash_hex,
                    &approver_identity,
                    ttl.as_secs(),
                )?;
                Ok(Decision::Allow)
            }
            ApprovalOutcome::Rejected { reason, decided_at } => {
                self.emit_approval_rejected(&req, &reason, decided_at)?;
                // Cache the deny so identical retries within TTL
                // short-circuit without re-asking the operator.
                let grant = ApprovalGrant::deny(tool_name, canonical_args, ttl, reason.clone());
                self.grant_table.insert(grant);
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

    /// Resolve the manifest's `tools.<class>.approval` policy block, if
    /// present. Returns `None` when no block is declared for the
    /// class, which the caller treats as the legacy halt-and-prompt
    /// (Validating) default.
    fn approval_policy_for(&self, class: &ToolClass) -> Option<&ApprovalPolicy> {
        let m = self.policy().manifest();
        match class {
            ToolClass::Filesystem => m.tools.filesystem.as_ref()?.approval.as_ref(),
            ToolClass::Network => m.tools.network.as_ref()?.approval.as_ref(),
            ToolClass::Exec => m.tools.exec.as_ref()?.approval.as_ref(),
            ToolClass::Mcp(server) => m
                .tools
                .mcp
                .iter()
                .find(|s| &s.server_name == server)?
                .approval
                .as_ref(),
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

    /// Emit an `approval_decision` entry for an advisory-tier dispatch
    /// (ADR-029 §"Risk-tiered approval scopes"). No operator was
    /// prompted; the entry is purely audit. Includes the canonical
    /// arg hash so auditors can reconstruct exactly what was
    /// auto-approved.
    fn emit_approval_decision_advisory(
        &mut self,
        tool_name: &str,
        canonical_args: &Value,
        resource_uri: &str,
        access_kind: &str,
        reasoning_step_id: Option<&str>,
    ) -> Result<()> {
        let arg_hash_hex = hex::encode(aegis_approval_gate::canonical_arg_hash(canonical_args));
        let agent_hash = self.agent_identity_hash();
        let session_id = self.session_id().to_string();
        let mut payload = Map::new();
        payload.insert(
            "decision".to_string(),
            Value::String("auto_advisory".to_string()),
        );
        payload.insert("toolName".to_string(), Value::String(tool_name.to_string()));
        payload.insert("grantArgHashHex".to_string(), Value::String(arg_hash_hex));
        payload.insert(
            "resourceUri".to_string(),
            Value::String(resource_uri.to_string()),
        );
        payload.insert(
            "accessType".to_string(),
            Value::String(access_kind.to_string()),
        );
        if let Some(rsid) = reasoning_step_id {
            payload.insert(
                "reasoningStepId".to_string(),
                Value::String(rsid.to_string()),
            );
        }
        self.ledger_writer_mut().append(Entry {
            session_id,
            entry_type: EntryType::ApprovalDecision,
            agent_identity_hash: agent_hash,
            timestamp: Utc::now(),
            payload,
        })?;
        Ok(())
    }

    /// Emit an `approval_decision` entry for an auto-consumed grant
    /// (ADR-029 §"Auto-consumption rules"). References the source
    /// grant id so an auditor can trace every silent retry back to
    /// the original operator decision.
    #[allow(clippy::too_many_arguments)]
    fn emit_approval_decision_auto_consumed(
        &mut self,
        tool_name: &str,
        resource_uri: &str,
        access_kind: &str,
        reasoning_step_id: Option<&str>,
        grant_id: &str,
        arg_hash_hex: &str,
        cached: &GrantDecision,
    ) -> Result<()> {
        let agent_hash = self.agent_identity_hash();
        let session_id = self.session_id().to_string();
        let outcome = match cached {
            GrantDecision::Allow => "auto_consumed_allow",
            GrantDecision::Deny { .. } => "auto_consumed_deny",
        };
        let mut payload = Map::new();
        payload.insert("decision".to_string(), Value::String(outcome.to_string()));
        payload.insert("toolName".to_string(), Value::String(tool_name.to_string()));
        payload.insert(
            "sourceGrantId".to_string(),
            Value::String(grant_id.to_string()),
        );
        payload.insert(
            "grantArgHashHex".to_string(),
            Value::String(arg_hash_hex.to_string()),
        );
        payload.insert(
            "resourceUri".to_string(),
            Value::String(resource_uri.to_string()),
        );
        payload.insert(
            "accessType".to_string(),
            Value::String(access_kind.to_string()),
        );
        if let Some(rsid) = reasoning_step_id {
            payload.insert(
                "reasoningStepId".to_string(),
                Value::String(rsid.to_string()),
            );
        }
        if let GrantDecision::Deny { reason } = cached {
            payload.insert("denyReason".to_string(), Value::String(reason.clone()));
        }
        self.ledger_writer_mut().append(Entry {
            session_id,
            entry_type: EntryType::ApprovalDecision,
            agent_identity_hash: agent_hash,
            timestamp: Utc::now(),
            payload,
        })?;
        Ok(())
    }

    /// Emit an `approval_decision` entry for a freshly-granted
    /// approval (ADR-029). Pairs with the existing `approval_granted`
    /// entry: that one records the operator's decision, this one
    /// records the issued grant so subsequent auto-consumes can chain
    /// to it via `sourceGrantId`.
    #[allow(clippy::too_many_arguments)]
    fn emit_approval_decision_granted(
        &mut self,
        tool_name: &str,
        resource_uri: &str,
        access_kind: &str,
        reasoning_step_id: Option<&str>,
        grant_id: &str,
        arg_hash_hex: &str,
        approver_identity: &str,
        ttl_seconds: u64,
    ) -> Result<()> {
        let agent_hash = self.agent_identity_hash();
        let session_id = self.session_id().to_string();
        let mut payload = Map::new();
        payload.insert(
            "decision".to_string(),
            Value::String("grant_issued".to_string()),
        );
        payload.insert("toolName".to_string(), Value::String(tool_name.to_string()));
        payload.insert(
            "sourceGrantId".to_string(),
            Value::String(grant_id.to_string()),
        );
        payload.insert(
            "grantArgHashHex".to_string(),
            Value::String(arg_hash_hex.to_string()),
        );
        payload.insert(
            "grantTtlSeconds".to_string(),
            Value::Number(ttl_seconds.into()),
        );
        payload.insert(
            "approverId".to_string(),
            Value::String(approver_identity.to_string()),
        );
        payload.insert(
            "resourceUri".to_string(),
            Value::String(resource_uri.to_string()),
        );
        payload.insert(
            "accessType".to_string(),
            Value::String(access_kind.to_string()),
        );
        if let Some(rsid) = reasoning_step_id {
            payload.insert(
                "reasoningStepId".to_string(),
                Value::String(rsid.to_string()),
            );
        }
        self.ledger_writer_mut().append(Entry {
            session_id,
            entry_type: EntryType::ApprovalDecision,
            agent_identity_hash: agent_hash,
            timestamp: Utc::now(),
            payload,
        })?;
        Ok(())
    }
}

/// Default grant TTL when `tools.<class>.approval.grant_ttl_seconds`
/// is not declared in the manifest. Matches the ADR-029 default of
/// 5 minutes.
const DEFAULT_GRANT_TTL_SECS: u64 = 300;

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

fn mcp_uri(server_name: &str, tool_name: &str) -> String {
    format!("mcp://{server_name}/{tool_name}")
}

/// Extract the value(s) a [`PreValidateClause`] points at from the
/// MCP tool-call payload (which is a free-form `serde_json::Value`).
///
/// - `arg: <key>` — `args[key]` must be a JSON string. Returns one
///   element.
/// - `arg_array: <key>` — `args[key]` must be a JSON array of
///   strings. Returns one element per array entry.
/// - Both unset / both set → schema bug (the JSON Schema's `oneOf`
///   normally catches this; we fail closed if it slips through).
///
/// Errors map to [`Error::McpPreValidateMalformedArg`] — the model's
/// tool call gets denied with a typed reason rather than crashing
/// the session.
fn extract_clause_values(
    args: &Value,
    server_name: &str,
    tool_name: &str,
    clause: &PreValidateClause,
) -> Result<Vec<String>> {
    let make_err = |arg: &str, reason: String| Error::McpPreValidateMalformedArg {
        server: server_name.to_string(),
        tool: tool_name.to_string(),
        arg: arg.to_string(),
        reason,
    };

    match (clause.arg.as_deref(), clause.arg_array.as_deref()) {
        (Some(key), None) => {
            let v = args.get(key).ok_or_else(|| {
                make_err(
                    key,
                    "required by pre_validate clause but missing from tool args".to_string(),
                )
            })?;
            let s = v
                .as_str()
                .ok_or_else(|| {
                    make_err(
                        key,
                        format!("expected JSON string, got {}", json_type_name(v)),
                    )
                })?
                .to_string();
            Ok(vec![s])
        }
        (None, Some(key)) => {
            let v = args.get(key).ok_or_else(|| {
                make_err(
                    key,
                    "required by pre_validate clause but missing from tool args".to_string(),
                )
            })?;
            let arr = v.as_array().ok_or_else(|| {
                make_err(
                    key,
                    format!(
                        "expected JSON array (arg_array clause), got {}",
                        json_type_name(v)
                    ),
                )
            })?;
            let mut out = Vec::with_capacity(arr.len());
            for (i, elem) in arr.iter().enumerate() {
                let s = elem.as_str().ok_or_else(|| {
                    make_err(
                        key,
                        format!(
                            "array element {i} is {}, expected string",
                            json_type_name(elem)
                        ),
                    )
                })?;
                out.push(s.to_string());
            }
            Ok(out)
        }
        (Some(_), Some(_)) => Err(make_err(
            "<both>",
            "pre_validate clause has both `arg` and `arg_array` set; schema oneOf is supposed to refuse this".to_string(),
        )),
        (None, None) => Err(make_err(
            "<neither>",
            "pre_validate clause has neither `arg` nor `arg_array`; schema oneOf is supposed to require one".to_string(),
        )),
    }
}

/// Friendly JSON type name for error messages.
fn json_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Parse a URL-shaped string into `(host, port)` for the
/// `network_outbound` pre-validate clause. Phase 1 supports:
///
/// - Full URL (`https://api.example.com:8443/foo`) → host +
///   explicit port if present, otherwise the scheme's default
///   (80 / 443) for `http` / `https`.
/// - Bare `host:port` (no scheme) — common when an MCP tool's
///   `target` arg is a host:port pair.
///
/// Anything else is a malformed-arg error. We refuse rather than
/// guessing — the manifest declared `kind: network_outbound`, so
/// the caller should be passing something parseable.
fn parse_pre_validate_url(
    raw: &str,
    server_name: &str,
    tool_name: &str,
    arg_name: &str,
) -> Result<(String, u16)> {
    let make_err = |reason: String| Error::McpPreValidateMalformedArg {
        server: server_name.to_string(),
        tool: tool_name.to_string(),
        arg: arg_name.to_string(),
        reason,
    };

    // Strip a leading scheme if present.
    let (default_port, after_scheme) = if let Some(rest) = raw.strip_prefix("https://") {
        (Some(443u16), rest)
    } else if let Some(rest) = raw.strip_prefix("http://") {
        (Some(80u16), rest)
    } else if raw.contains("://") {
        // Some other scheme — refuse rather than misinterpret.
        return Err(make_err(format!(
            "unsupported URL scheme in {raw:?} (Phase 1 supports http/https or bare host:port)"
        )));
    } else {
        (None, raw)
    };

    // Split off path / query / fragment so they don't leak into the
    // host parser.
    let host_port = after_scheme
        .split_once('/')
        .map(|(hp, _)| hp)
        .unwrap_or(after_scheme);
    let host_port = host_port
        .split_once('?')
        .map(|(hp, _)| hp)
        .unwrap_or(host_port);

    if host_port.is_empty() {
        return Err(make_err(format!("empty host in {raw:?}")));
    }

    if let Some((host, port_str)) = host_port.rsplit_once(':') {
        let port: u16 = port_str.parse().map_err(|_| {
            make_err(format!(
                "port component {port_str:?} in {raw:?} is not a valid u16"
            ))
        })?;
        if host.is_empty() {
            return Err(make_err(format!("empty host before port in {raw:?}")));
        }
        Ok((host.to_string(), port))
    } else {
        // No `:port` — only valid for the http/https schemes where
        // a default exists.
        let port = default_port.ok_or_else(|| {
            make_err(format!(
                "no port in {raw:?} and no scheme to imply a default (try host:port)"
            ))
        })?;
        Ok((host_port.to_string(), port))
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
