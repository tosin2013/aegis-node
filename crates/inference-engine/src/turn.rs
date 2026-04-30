//! `Session::run_turn` driver — the LLM-B integration point.
//!
//! Per LLM-B / [issue #71](https://github.com/tosin2013/aegis-node/issues/71)
//! and ADR-014. One call:
//!
//! 1. Build [`InferRequest`] from the user input + the manifest's MCP
//!    tool catalog.
//! 2. Call the attached [`LoadedModel::infer`] (the LLM-B trait).
//! 3. Emit one F5 [`ReasoningStep`] ledger entry from the response —
//!    the reasoning text plus `tool_selected` populated from the
//!    parsed tool calls.
//! 4. For each [`ToolCall`] the model emitted, route through the
//!    appropriate `mediate_*` method on `Session`. LLM-B routes only
//!    MCP tools (per the issue's E2E acceptance); filesystem / network
//!    / exec dispatch land in a follow-up.
//! 5. Return a [`TurnOutcome`] capturing assistant text + per-call
//!    outcomes for the caller.
//!
//! The driver is intentionally narrow: the runtime contract stays
//! "every side-effect routes through `mediate_*`," so once the model
//! emits a tool call we lean on the existing per-tool-call mediator.
//! No new policy gates here.

use crate::backend::{InferRequest, ToolCall, ToolDecl};
use crate::error::{Error, Result};
use crate::session::Session;

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
    /// Tool name as the model emitted it (`<server>__<tool>` for MCP).
    pub name: String,
    /// Result of the dispatch.
    pub result: ToolCallResult,
}

/// Three terminal states for one tool call.
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
    /// doesn't fit the `<server>__<tool>` convention LLM-B expects.
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
        let tools = self.mcp_tool_catalog();
        let messages = vec![crate::backend::ChatMessage {
            role: crate::backend::ChatRole::User,
            content: user_message.to_string(),
        }];

        let response = {
            let model = self
                .loaded_model
                .as_mut()
                .ok_or(Error::NoBackendConfigured)?;
            model.infer(InferRequest { messages, tools })?
        };

        // Emit one F5 reasoning step capturing the model's stated
        // chain — input + reasoning + tools considered + selected.
        let tools_considered: Vec<String> = self
            .policy()
            .manifest()
            .tools
            .mcp
            .iter()
            .flat_map(|s| {
                s.allowed_tools
                    .iter()
                    .map(move |t| format_mcp_name(&s.server_name, t))
            })
            .collect();
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

    /// Build the LLM-B tool catalog from the manifest's MCP grants.
    /// Names are formatted as `<server>__<tool>` so the model's
    /// emission can be split unambiguously by [`split_mcp_name`].
    fn mcp_tool_catalog(&self) -> Vec<ToolDecl> {
        let mut decls = Vec::new();
        for server in &self.policy().manifest().tools.mcp {
            for tool_name in &server.allowed_tools {
                decls.push(ToolDecl {
                    name: format_mcp_name(&server.server_name, tool_name),
                    description: format!(
                        "MCP tool {tool_name} on server {} (URI: {})",
                        server.server_name, server.server_uri
                    ),
                    // No schema in the manifest yet — pass an open
                    // object so the model isn't constrained. The MCP
                    // server returns its own schema at handshake time;
                    // surfacing that into the catalog lands in a
                    // follow-up alongside MCP discovery.
                    arguments_schema: serde_json::json!({"type": "object"}),
                });
            }
        }
        decls
    }

    /// Route one [`ToolCall`] through the existing per-tool mediator.
    /// LLM-B handles MCP tools only; filesystem / network / exec
    /// dispatch lands in a follow-up that introduces a richer name
    /// scheme.
    fn dispatch_tool_call(
        &mut self,
        call: ToolCall,
        reasoning_step_id: Option<&str>,
    ) -> Result<ToolCallOutcome> {
        let Some((server, tool)) = split_mcp_name(&call.name) else {
            return Ok(ToolCallOutcome {
                name: call.name.clone(),
                result: ToolCallResult::Unroutable(format!(
                    "tool name {:?} not in <server>__<tool> shape",
                    call.name
                )),
            });
        };
        let result =
            match self.mediate_mcp_tool_call(server, tool, call.arguments, reasoning_step_id) {
                Ok(value) => ToolCallResult::Success(value),
                Err(Error::Denied { reason }) => ToolCallResult::Denied(reason),
                Err(Error::RequireApproval { reason }) => ToolCallResult::RequiresApproval(reason),
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
        let qualified = format_mcp_name("filesystem", "read_file");
        assert_eq!(qualified, "filesystem__read_file");
        let (server, tool) = split_mcp_name(&qualified).unwrap();
        assert_eq!(server, "filesystem");
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
}
