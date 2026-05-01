//! `Session::run_turn` driver — the LLM-B integration point.
//!
//! Per LLM-B / [issue #71](https://github.com/tosin2013/aegis-node/issues/71)
//! and [issue #92](https://github.com/tosin2013/aegis-node/issues/92).
//! One call:
//!
//! 1. Build [`InferRequest`] from the user input + the manifest's tool
//!    catalog (MCP servers + native filesystem / network / exec
//!    grants).
//! 2. Call the attached [`LoadedModel::infer`] (the LLM-B trait).
//! 3. Emit one F5 [`ReasoningStep`] ledger entry from the response —
//!    the reasoning text plus `tool_selected` populated from the
//!    parsed tool calls.
//! 4. For each [`ToolCall`] the model emitted, route through the
//!    appropriate `mediate_*` method on `Session`.
//! 5. Return a [`TurnOutcome`] capturing assistant text + per-call
//!    outcomes for the caller.
//!
//! ## Tool-name routing
//!
//! Tools are named `<namespace>__<tool>`. The dispatcher recognizes
//! three reserved namespaces that map to native mediators:
//!
//! | Namespace | Tools | Mediator |
//! |---|---|---|
//! | `filesystem` | `read`, `write`, `delete` | [`Session::mediate_filesystem_read`] / `_write` / `_delete` |
//! | `network` | `connect` | [`Session::mediate_network_connect`] |
//! | `exec` | `run` | [`Session::mediate_exec`] |
//!
//! Any other namespace is treated as an MCP server name and dispatched
//! through [`Session::mediate_mcp_tool_call`]. A manifest that
//! declares an MCP server whose `server_name` shadows one of these
//! native namespaces is rejected at [`Session::boot`] with
//! [`Error::ReservedMcpServerName`] — the conflict is loud, not silent.

use std::path::PathBuf;

use crate::backend::{InferRequest, ToolCall, ToolDecl};
use crate::error::{Error, Result};
use crate::session::Session;

/// Reserved namespace names. An MCP server in `tools.mcp[]` whose
/// `server_name` matches any of these collides with native dispatch
/// and is refused at boot.
pub const RESERVED_NATIVE_NAMESPACES: &[&str] = &["filesystem", "network", "exec"];

/// Outcome of one [`Session::run_turn`] call. Captures every
/// observable side-effect the caller might want to act on (logs,
/// retries, halt decisions). The ledger holds the canonical record;
/// this struct is the in-process echo.
#[derive(Debug, Clone)]
pub struct TurnOutcome {
    /// Assistant text the model produced — `Some` when the model
    /// emitted free-text reasoning intended for the user, `None`
    /// when it went straight to tool calls.
    pub assistant_text: Option<String>,
    /// Per-tool-call outcome, in emission order.
    pub tool_calls: Vec<ToolCallOutcome>,
}

/// Outcome of one dispatched [`ToolCall`].
#[derive(Debug, Clone)]
pub struct ToolCallOutcome {
    /// Tool name as the model emitted it (`<namespace>__<tool>`).
    pub name: String,
    /// Result of the dispatch.
    pub result: ToolCallResult,
}

/// Four terminal states for one tool call.
#[derive(Debug, Clone)]
pub enum ToolCallResult {
    /// Mediator allowed and the upstream tool returned `value`.
    Success(serde_json::Value),
    /// Mediator denied — `reason` is the policy / runtime reason that
    /// already lives in the ledger as a Violation entry.
    Denied(String),
    /// Mediator demanded approval and the call short-circuited (the
    /// approval channel either timed out, was rejected, or wasn't
    /// configured). `reason` is the same reason in the ledger.
    RequiresApproval(String),
    /// The tool call wasn't routable — the model emitted a name that
    /// doesn't fit the `<namespace>__<tool>` convention, or named a
    /// native namespace tool the runtime doesn't implement, or the
    /// arguments were malformed.
    Unroutable(String),
}

impl Session {
    /// Run one chat turn end-to-end: build request → infer → emit
    /// reasoning → dispatch tool calls → return outcome.
    ///
    /// Errors:
    /// - [`Error::NoBackendConfigured`] when the session was booted
    ///   without [`Self::with_loaded_model`].
    /// - [`Error::BackendInfer`] when the inference itself fails.
    /// - Any error from `mediate_*` propagates only if it would also
    ///   propagate from the legacy fixed-script `run` path (e.g.,
    ///   identity rebind violation). Per-call denials and approval
    ///   refusals are captured into [`TurnOutcome`] rather than
    ///   short-circuiting the turn — the agent saw the refusal, the
    ///   ledger has the Violation, the next turn can adapt.
    pub fn run_turn(&mut self, user_message: &str) -> Result<TurnOutcome> {
        let tools = self.tool_catalog();
        let messages = vec![crate::backend::ChatMessage {
            role: crate::backend::ChatRole::User,
            content: user_message.to_string(),
        }];

        let response = {
            let model = self
                .loaded_model
                .as_mut()
                .ok_or(Error::NoBackendConfigured)?;
            model.infer(InferRequest {
                messages,
                tools: tools.clone(),
            })?
        };

        // Emit one F5 reasoning step capturing the model's stated
        // chain — input + reasoning + tools considered + selected.
        let tools_considered: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
        let tool_selected = response.tool_calls.first().map(|c| c.name.clone());
        let step_uuid = self.record_reasoning_step(
            user_message,
            &response.reasoning,
            tools_considered,
            tool_selected,
        )?;
        let step_id = step_uuid.to_string();

        // Dispatch each tool call. We capture per-call outcomes rather
        // than short-circuit the loop on Denied/RequireApproval —
        // those are normal runtime conditions, not turn-fatal errors.
        let mut outcomes = Vec::with_capacity(response.tool_calls.len());
        for call in response.tool_calls {
            let outcome = self.dispatch_tool_call(call, Some(&step_id))?;
            outcomes.push(outcome);
        }

        Ok(TurnOutcome {
            assistant_text: response.assistant_text,
            tool_calls: outcomes,
        })
    }

    /// Build the LLM-B tool catalog: native filesystem / network /
    /// exec entries (when the manifest grants them) plus one entry
    /// per `tools.mcp[].allowed_tools` member.
    fn tool_catalog(&self) -> Vec<ToolDecl> {
        let mut decls = Vec::new();
        let manifest = self.policy().manifest();

        // Native filesystem grants.
        if let Some(fs) = manifest.tools.filesystem.as_ref() {
            if !fs.read.is_empty() {
                decls.push(ToolDecl {
                    name: "filesystem__read".to_string(),
                    description: format!(
                        "Read a file. Allowed paths (or paths under them): {}",
                        fs.read.join(", ")
                    ),
                    arguments_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string", "description": "Absolute path of the file to read."}
                        },
                        "required": ["path"],
                    }),
                });
            }
            if !fs.write.is_empty() || !manifest.write_grants.is_empty() {
                decls.push(ToolDecl {
                    name: "filesystem__write".to_string(),
                    description: format!(
                        "Write contents to a file. Coverage: {} (broad) and {} write_grant(s) (narrow).",
                        if fs.write.is_empty() { "<none>".to_string() } else { fs.write.join(", ") },
                        manifest.write_grants.len(),
                    ),
                    arguments_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"},
                            "contents": {"type": "string"}
                        },
                        "required": ["path", "contents"],
                    }),
                });
            }
        }

        // Network outbound. The mediator deny-by-default policy still
        // applies; emit the catalog entry whenever an `outbound`
        // policy is set so the model knows to attempt.
        if let Some(net) = manifest.tools.network.as_ref() {
            if net.outbound.is_some() {
                decls.push(ToolDecl {
                    name: "network__connect".to_string(),
                    description: "Open an outbound network connection. Subject to tools.network.outbound policy.".to_string(),
                    arguments_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "host": {"type": "string"},
                            "port": {"type": "integer"},
                            "protocol": {"type": "string", "enum": ["tcp", "udp"]}
                        },
                        "required": ["host", "port"],
                    }),
                });
            }
        }

        // Exec grants.
        if !manifest.exec_grants.is_empty() {
            let allowed: Vec<&str> = manifest
                .exec_grants
                .iter()
                .map(|g| g.program.as_str())
                .collect();
            decls.push(ToolDecl {
                name: "exec__run".to_string(),
                description: format!(
                    "Run a permitted program. Allowed programs: {}",
                    allowed.join(", ")
                ),
                arguments_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "program": {"type": "string"},
                        "args": {"type": "array", "items": {"type": "string"}}
                    },
                    "required": ["program"],
                }),
            });
        }

        // MCP catalog entries. Per ADR-024, allowed_tools is now a
        // union — entries may be bare strings or objects with
        // pre_validate clauses; the catalog only surfaces names so
        // we ask each entry for its name regardless of shape.
        for server in &manifest.tools.mcp {
            for entry in &server.allowed_tools {
                let tool_name = entry.name();
                decls.push(ToolDecl {
                    name: format_mcp_name(&server.server_name, tool_name),
                    description: format!(
                        "MCP tool {tool_name} on server {} (URI: {})",
                        server.server_name, server.server_uri
                    ),
                    arguments_schema: serde_json::json!({"type": "object"}),
                });
            }
        }

        decls
    }

    /// Route one [`ToolCall`] through the appropriate per-tool
    /// mediator. Native-namespace tools (`filesystem__*`,
    /// `network__connect`, `exec__run`) dispatch directly; everything
    /// else is treated as an MCP server-qualified name.
    fn dispatch_tool_call(
        &mut self,
        call: ToolCall,
        reasoning_step_id: Option<&str>,
    ) -> Result<ToolCallOutcome> {
        let Some((namespace, tool)) = split_mcp_name(&call.name) else {
            return Ok(ToolCallOutcome {
                name: call.name.clone(),
                result: ToolCallResult::Unroutable(format!(
                    "tool name {:?} not in <namespace>__<tool> shape",
                    call.name
                )),
            });
        };

        let dispatch_result = match namespace {
            "filesystem" => {
                self.dispatch_native_filesystem(tool, &call.arguments, reasoning_step_id)
            }
            "network" => self.dispatch_native_network(tool, &call.arguments, reasoning_step_id),
            "exec" => self.dispatch_native_exec(tool, &call.arguments, reasoning_step_id),
            _ => self.dispatch_mcp(namespace, tool, call.arguments.clone(), reasoning_step_id),
        };

        let result = match dispatch_result {
            Ok(value) => ToolCallResult::Success(value),
            Err(Error::Denied { reason }) => ToolCallResult::Denied(reason),
            Err(Error::RequireApproval { reason }) => ToolCallResult::RequiresApproval(reason),
            // Native-tool unroutable (unknown tool / malformed args)
            // surfaces as a typed error variant we map to ToolCallResult.
            Err(Error::UnroutableToolCall { name }) => {
                ToolCallResult::Unroutable(format!("native dispatch refused: {name}"))
            }
            // Identity rebind / I/O / other errors propagate — they
            // already wrote a Violation entry where applicable, and
            // the mediator's contract is "halt the run."
            Err(other) => return Err(other),
        };
        Ok(ToolCallOutcome {
            name: call.name,
            result,
        })
    }

    fn dispatch_mcp(
        &mut self,
        server: &str,
        tool: &str,
        args: serde_json::Value,
        reasoning_step_id: Option<&str>,
    ) -> Result<serde_json::Value> {
        self.mediate_mcp_tool_call(server, tool, args, reasoning_step_id)
    }

    fn dispatch_native_filesystem(
        &mut self,
        tool: &str,
        args: &serde_json::Value,
        reasoning_step_id: Option<&str>,
    ) -> Result<serde_json::Value> {
        let path = path_arg(args, "path")?;
        match tool {
            "read" => {
                let bytes = self.mediate_filesystem_read(&path, reasoning_step_id)?;
                Ok(read_response(&bytes))
            }
            "write" => {
                let contents = string_arg(args, "contents")?;
                self.mediate_filesystem_write(&path, contents.as_bytes(), reasoning_step_id)?;
                Ok(serde_json::json!({"path": path.display().to_string(), "bytes": contents.len()}))
            }
            "delete" => {
                self.mediate_filesystem_delete(&path, reasoning_step_id)?;
                Ok(serde_json::json!({"path": path.display().to_string(), "deleted": true}))
            }
            other => Err(Error::UnroutableToolCall {
                name: format!("filesystem__{other}"),
            }),
        }
    }

    fn dispatch_native_network(
        &mut self,
        tool: &str,
        args: &serde_json::Value,
        reasoning_step_id: Option<&str>,
    ) -> Result<serde_json::Value> {
        match tool {
            "connect" => {
                let host = string_arg(args, "host")?;
                let port = u16_arg(args, "port")?;
                let proto_str = args
                    .get("protocol")
                    .and_then(|v| v.as_str())
                    .unwrap_or("tcp");
                let proto = match proto_str {
                    "tcp" => aegis_policy::NetworkProto::Tcp,
                    "udp" => aegis_policy::NetworkProto::Udp,
                    other => {
                        return Err(Error::UnroutableToolCall {
                            name: format!("network__connect (unknown protocol {other:?})"),
                        });
                    }
                };
                // We don't actually use the returned TcpStream — the
                // demo / agent observes "the connect was allowed" via
                // the F4 Access entry the mediator emits. The stream
                // closes when this scope drops.
                let _stream =
                    self.mediate_network_connect(&host, port, proto, reasoning_step_id)?;
                Ok(
                    serde_json::json!({"host": host, "port": port, "protocol": proto_str, "connected": true}),
                )
            }
            other => Err(Error::UnroutableToolCall {
                name: format!("network__{other}"),
            }),
        }
    }

    fn dispatch_native_exec(
        &mut self,
        tool: &str,
        args: &serde_json::Value,
        reasoning_step_id: Option<&str>,
    ) -> Result<serde_json::Value> {
        match tool {
            "run" => {
                let program = path_arg(args, "program")?;
                let exec_args: Vec<String> = args
                    .get("args")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                let arg_refs: Vec<&str> = exec_args.iter().map(|s| s.as_str()).collect();
                let output = self.mediate_exec(&program, &arg_refs, reasoning_step_id)?;
                Ok(serde_json::json!({
                    "program": program.display().to_string(),
                    "exit": output.status.code(),
                    "stdout": String::from_utf8_lossy(&output.stdout),
                    "stderr": String::from_utf8_lossy(&output.stderr),
                }))
            }
            other => Err(Error::UnroutableToolCall {
                name: format!("exec__{other}"),
            }),
        }
    }
}

/// Translate the bytes returned by `mediate_filesystem_read` into a
/// JSON-friendly response. Tries UTF-8 first (the common case for
/// agent-readable docs); falls back to a placeholder when the bytes
/// aren't text. Either way the byte count is reported so the model
/// has size context.
fn read_response(bytes: &[u8]) -> serde_json::Value {
    let len = bytes.len();
    match std::str::from_utf8(bytes) {
        Ok(s) => serde_json::json!({"contents": s, "bytes": len}),
        Err(_) => serde_json::json!({"contents": null, "bytes": len, "binary": true}),
    }
}

fn path_arg(args: &serde_json::Value, key: &str) -> Result<PathBuf> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .ok_or_else(|| Error::UnroutableToolCall {
            name: format!("missing or non-string {key:?} argument"),
        })
}

fn string_arg(args: &serde_json::Value, key: &str) -> Result<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| Error::UnroutableToolCall {
            name: format!("missing or non-string {key:?} argument"),
        })
}

fn u16_arg(args: &serde_json::Value, key: &str) -> Result<u16> {
    let raw = args
        .get(key)
        .and_then(|v| v.as_u64())
        .ok_or_else(|| Error::UnroutableToolCall {
            name: format!("missing or non-integer {key:?} argument"),
        })?;
    u16::try_from(raw).map_err(|_| Error::UnroutableToolCall {
        name: format!("{key:?} argument out of u16 range: {raw}"),
    })
}

/// Format an MCP tool's qualified name for the LLM catalog.
pub(crate) fn format_mcp_name(server: &str, tool: &str) -> String {
    format!("{server}__{tool}")
}

/// Inverse of [`format_mcp_name`]. Returns `None` when the name
/// doesn't carry the `__` separator.
pub(crate) fn split_mcp_name(name: &str) -> Option<(&str, &str)> {
    let (server, tool) = name.split_once("__")?;
    if server.is_empty() || tool.is_empty() {
        return None;
    }
    Some((server, tool))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn format_round_trips_through_split() {
        let qualified = format_mcp_name("filesystem-mcp", "read_file");
        assert_eq!(qualified, "filesystem-mcp__read_file");
        let (server, tool) = split_mcp_name(&qualified).unwrap();
        assert_eq!(server, "filesystem-mcp");
        assert_eq!(tool, "read_file");
    }

    #[test]
    fn split_rejects_unqualified_name() {
        assert_eq!(split_mcp_name("just_a_tool"), None);
    }

    #[test]
    fn split_rejects_empty_components() {
        assert_eq!(split_mcp_name("__tool"), None);
        assert_eq!(split_mcp_name("server__"), None);
    }

    #[test]
    fn reserved_namespaces_match_native_dispatch_branches() {
        // Sanity: the constant the boot check uses agrees with the
        // namespaces dispatch_tool_call switches on. If you add a
        // native namespace, both lists need to grow together.
        let mut got: Vec<&str> = RESERVED_NATIVE_NAMESPACES.to_vec();
        got.sort();
        assert_eq!(got, vec!["exec", "filesystem", "network"]);
    }
}
