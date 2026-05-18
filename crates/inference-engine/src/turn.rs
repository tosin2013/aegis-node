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

use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use aegis_ledger_writer::LedgerSchemaVersion;
use sha2::{Digest as _, Sha256};

use crate::adversarial::{ClassifierVerdict, ToolOrigin};
use crate::backend::{ChatMessage, ChatRole, InferRequest, ToolCall, ToolDecl};
use crate::error::{Error, Result};
use crate::session::Session;

/// Reserved namespace names. An MCP server in `tools.mcp[]` whose
/// `server_name` matches any of these collides with native dispatch
/// and is refused at boot.
pub const RESERVED_NATIVE_NAMESPACES: &[&str] = &["filesystem", "network", "exec"];

/// Triple-Bound Circuit Breaker config for [`Session::run`] (ADR-025).
///
/// Each bound is independent — whichever trips first halts the loop
/// and writes a `TurnCapExceeded` Violation to the F9 ledger before
/// the driver returns [`Error::TurnCapExceeded`].
#[derive(Debug, Clone, Copy)]
pub struct TurnLimits {
    /// Hard cap on model invocations per session. ADR-025 default: 10.
    pub max_turns: u32,
    /// Cumulative token budget across all turns. Backends that don't
    /// report usage ([`crate::InferResponse::tokens_used`] == None)
    /// contribute 0 to the accumulator, so this bound only trips on
    /// backends with wired usage reporting. ADR-025 default: per-backend.
    /// The library default of `u64::MAX` is "effectively unbounded" —
    /// callers (the CLI) set it explicitly.
    pub max_tokens: u64,
    /// Wallclock budget from [`Session::run`] entry. ADR-025 default: 300s.
    pub max_seconds: u64,
}

impl Default for TurnLimits {
    fn default() -> Self {
        Self {
            max_turns: 10,
            max_tokens: u64::MAX,
            max_seconds: 300,
        }
    }
}

/// Which bound of the Triple-Bound Circuit Breaker tripped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnCapKind {
    /// `max_turns` exceeded.
    Turns,
    /// `max_tokens` exceeded.
    Tokens,
    /// `max_seconds` (wallclock) exceeded.
    Wallclock,
}

impl fmt::Display for TurnCapKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TurnCapKind::Turns => "turns",
            TurnCapKind::Tokens => "tokens",
            TurnCapKind::Wallclock => "wallclock",
        };
        f.write_str(s)
    }
}

/// Why [`Session::run`] stopped. Capped termination surfaces as
/// [`Error::TurnCapExceeded`] on the [`Result`]; [`Session::run`]
/// only returns this variant on clean termination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTermination {
    /// The most recent turn emitted no tool calls. The assistant text
    /// from that turn is the final response. ADR-025 §"Loop shape"
    /// case "tool_call list is empty".
    Done,
}

/// Aggregate outcome of [`Session::run`]: per-turn captures + the
/// reason the loop halted + cumulative usage counters.
#[derive(Debug, Clone)]
pub struct SessionRunResult {
    /// Per-turn outcomes in chronological order. Includes the
    /// final turn (the one whose empty tool-call list triggered
    /// the clean termination).
    pub turns: Vec<TurnOutcome>,
    /// Why the loop ended. On clean termination this is
    /// [`SessionTermination::Done`]. Capped termination doesn't
    /// reach this struct — see [`Error::TurnCapExceeded`].
    pub termination: SessionTermination,
    /// Sum of [`crate::InferResponse::tokens_used`] across all
    /// completed turns (treating `None` as zero).
    pub tokens_consumed: u64,
    /// Wallclock from the start of [`Session::run`] to the
    /// turn that produced the final assistant text.
    pub wallclock: Duration,
}

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
    /// UUIDv7 of the F5 reasoning-step ledger entry this turn
    /// produced. The same id appears in
    /// [F9](../../docs/adrs/011-hash-chained-tamper-evident-ledger.md)
    /// as the step's `id` and on every per-call ledger entry the
    /// dispatcher emits during the turn (each carries
    /// `reasoning_step_id`). Surfaced here so callers — the WebUI
    /// chat surface (ADR-031), evidence-pack generators, replay
    /// viewers — can anchor their per-turn UI to the cryptographic
    /// trail in the F9 ledger without re-reading the file.
    pub reasoning_step_id: String,
}

/// Outcome of one dispatched [`ToolCall`]. Captures the model's
/// emitted args alongside the mediator's terminal result so callers
/// have the full call signature in-process, not just the verdict.
#[derive(Debug, Clone)]
pub struct ToolCallOutcome {
    /// Tool name as the model emitted it (`<namespace>__<tool>`).
    pub name: String,
    /// Args the model passed, preserved verbatim from the
    /// [`ToolCall::arguments`] the mediator received. The runtime
    /// validates these against the manifest's allowlist + ADR-024
    /// `pre_validate` clauses before dispatch; surfacing them on
    /// the outcome lets the WebUI render them inside its inline
    /// tool-call cards (ADR-031 §"Inline tool-call cards").
    pub arguments: serde_json::Value,
    /// Result of the dispatch.
    pub result: ToolCallResult,
    /// Adversarial pre-filter verdict (ADR-028). `Some(verdict)`
    /// when this outcome was produced by the multi-turn driver
    /// ([`Session::run`]) — the classifier is part of that path's
    /// re-injection guard. `None` when produced by the single-turn
    /// [`Session::run_turn`] path, which doesn't re-inject tool
    /// results into a follow-up prompt and thus doesn't need the
    /// gate. The WebUI uses this for the "suspected injection"
    /// badge described in ADR-028 §"Ledger integration".
    pub classifier_verdict: Option<crate::adversarial::ClassifierVerdict>,
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
        let messages = vec![ChatMessage {
            role: ChatRole::User,
            content: user_message.to_string(),
        }];
        // Single-turn legacy path does not emit v2 per-turn entries —
        // turn_number is None to signal that to `run_one_turn`.
        let (outcome, _tokens) = self.run_one_turn(&messages, user_message, None)?;
        Ok(outcome)
    }

    /// Drive a session through up to `limits.max_turns` LLM-B
    /// invocations, accumulating tool results into the context window
    /// for the next turn, until the model emits a turn with no tool
    /// calls (clean termination) or one of the Triple-Bound Circuit
    /// Breaker bounds trips (capped termination).
    ///
    /// Implements [ADR-025](../../docs/adrs/025-multi-turn-agent-loop-with-triple-bound-circuit-breaker.md).
    ///
    /// **Clean termination** returns `Ok(SessionRunResult)` with every
    /// turn's outcome in order.
    ///
    /// **Capped termination** writes a `TurnCapExceeded` Violation entry
    /// to the F9 ledger and returns [`Error::TurnCapExceeded`]. The
    /// ledger left on disk contains every fully-completed turn before
    /// the cap fired — the "partial F9 ledger" promise from ADR-025 §3.
    /// Callers parse the ledger (via `aegis verify` or directly) to
    /// reconstruct what happened before the halt.
    ///
    /// ## Message-history caveat
    ///
    /// Between turns the driver accumulates:
    /// - the original user message,
    /// - one [`ChatRole::Assistant`] message per turn carrying the
    ///   model's reasoning + assistant text,
    /// - one [`ChatRole::Tool`] message per dispatched call carrying a
    ///   JSON-shaped `{"tool": name, "args": ..., "result": ...}` body.
    ///
    /// The exact serialization is chat-template-dependent — Gemma 4 and
    /// other production backends benefit from richer tool-call /
    /// tool-result markers than `ChatMessage` exposes today. Lifting
    /// the message shape (per-message `tool_call_id`, `tool_name`) is
    /// tracked under ADR-026 + #182.
    pub fn run(&mut self, prompt: &str, limits: TurnLimits) -> Result<SessionRunResult> {
        let started = Instant::now();
        let mut messages = vec![ChatMessage {
            role: ChatRole::User,
            content: prompt.to_string(),
        }];
        let mut turns: Vec<TurnOutcome> = Vec::with_capacity(limits.max_turns as usize);
        let mut tokens_consumed: u64 = 0;
        let v2 = self.schema_version() == LedgerSchemaVersion::V2;

        // The driver runs as a for-loop with explicit cap checks at
        // the top so the bounds are visible without grep'ing for
        // mid-loop branches. ADR-025 §"Loop shape" is the spec.
        for turn_index in 1..=limits.max_turns {
            // Wallclock cap is checked before invoking the backend so
            // a hung tool from the previous turn can't push the model
            // over the bound; ditto a slow backend prompt-eval.
            let elapsed = started.elapsed();
            if elapsed.as_secs() >= limits.max_seconds {
                return Err(self.emit_cap_and_build_err(
                    TurnCapKind::Wallclock,
                    turn_index,
                    &limits,
                    tokens_consumed,
                    elapsed.as_secs_f64(),
                )?);
            }
            // Token cap on cumulative — `tokens_used` per turn is
            // optional ([`InferResponse::tokens_used`]). Backends that
            // don't report contribute 0; bound never trips on them.
            if tokens_consumed >= limits.max_tokens {
                return Err(self.emit_cap_and_build_err(
                    TurnCapKind::Tokens,
                    turn_index,
                    &limits,
                    tokens_consumed,
                    elapsed.as_secs_f64(),
                )?);
            }

            // v2 turn_start lands *before* `run_one_turn` so the
            // reasoning_step + tool_call/tool_result entries the inner
            // dispatch emits all parent-by-position to this turn_start
            // in the ledger stream. ADR-030: mint the per-turn SVID
            // here so the audience + thumbprint can be recorded in the
            // turn_start payload; the SVID is dropped at turn_end.
            if v2 {
                let ctx_hex = sha256_hex_of_messages(&messages);
                let audience = format!("aegis-turn://{}/{}", self.session_id(), turn_index);
                let remaining = limits
                    .max_seconds
                    .saturating_sub(started.elapsed().as_secs());
                let thumbprint = self.issue_turn_svid(&audience, remaining)?;
                self.write_turn_start(turn_index, &ctx_hex, &thumbprint, &audience)?;
            }

            let (mut outcome, used) = self.run_one_turn(&messages, prompt, Some(turn_index))?;
            let tokens_this_turn = used.unwrap_or(0);
            tokens_consumed = tokens_consumed.saturating_add(tokens_this_turn);
            let no_tool_calls = outcome.tool_calls.is_empty();

            // ADR-028 adversarial pre-filter — classify every tool
            // result *before* it can influence the next turn. Clean
            // results pass through unchanged; flagged ones are
            // wrapped in the `<aegis-system-warning>` block when we
            // build the `Tool` history message below, and a
            // `AdversarialContent` Violation is written to the F9
            // ledger.
            //
            // Collected into a parallel Vec so we keep `outcome`
            // immutable through the borrow-check dance with `self`.
            let classifier = self.adversarial_classifier.clone();
            let classifier_name = classifier.name();
            let mut classified: Vec<(ClassifierVerdict, ToolOrigin)> =
                Vec::with_capacity(outcome.tool_calls.len());
            for tc in &outcome.tool_calls {
                let origin = origin_of(&tc.name);
                let body_bytes = tool_call_result_to_value(&tc.result).to_string();
                let verdict = classifier.classify(body_bytes.as_bytes(), &origin);
                classified.push((verdict, origin));
            }
            // Emit ledger entries for flagged results before touching
            // the outcome (writes need `&mut self`).
            for ((verdict, origin), _tc) in classified.iter().zip(outcome.tool_calls.iter()) {
                if verdict.flagged() {
                    self.write_adversarial_violation(verdict, classifier_name, origin)?;
                }
            }
            // Now stamp each outcome with its verdict for downstream
            // consumers (WebUI, evidence-pack generator).
            for (tc, (verdict, _origin)) in outcome.tool_calls.iter_mut().zip(classified.iter()) {
                tc.classifier_verdict = Some(verdict.clone());
            }

            // Accumulate assistant + tool messages for the next turn.
            // The Assistant message carries the model's reasoning AND
            // the final assistant text (when present); we deliberately
            // omit a separate per-turn tool-call structured block — the
            // backend already parsed `tool_calls` and the tool-result
            // messages below carry the names back.
            let assistant_content = match &outcome.assistant_text {
                Some(text) => text.clone(),
                None => String::new(),
            };
            if !assistant_content.is_empty() {
                messages.push(ChatMessage {
                    role: ChatRole::Assistant,
                    content: assistant_content,
                });
            }
            for (tc, (verdict, origin)) in outcome.tool_calls.iter().zip(classified.iter()) {
                let body_value = serde_json::json!({
                    "tool": tc.name,
                    "args": tc.arguments,
                    "result": tool_call_result_to_value(&tc.result),
                });
                let body_str = body_value.to_string();
                let content = if verdict.flagged() {
                    // ADR-028 §"Sanitize, don't drop" — wrap, never
                    // strip. The wrapper escapes any inner
                    // `<aegis-system-warning>` so a forged inner
                    // wrapper can't fool the model.
                    crate::adversarial::wrap_flagged(
                        body_str.as_bytes(),
                        verdict,
                        origin,
                        classifier_name,
                    )
                } else {
                    body_str
                };
                messages.push(ChatMessage {
                    role: ChatRole::Tool,
                    content,
                });
            }
            turns.push(outcome);

            // v2 turn_end after the dispatch round + adversarial
            // emissions, before the loop control decides whether to
            // re-enter. On clean termination we return *after*
            // emitting turn_end so the ledger always brackets the
            // final turn. ADR-030: drop the per-turn SVID — it lives
            // only for the duration of the turn that issued it.
            if v2 {
                let wallclock_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
                self.write_turn_end(turn_index, None, used, tokens_consumed, wallclock_ms)?;
                self.drop_turn_svid();
            }

            if no_tool_calls {
                // Clean termination: the model produced text and no
                // tool calls — the task is its own answer.
                return Ok(SessionRunResult {
                    turns,
                    termination: SessionTermination::Done,
                    tokens_consumed,
                    wallclock: started.elapsed(),
                });
            }
        }

        // Fell out of the loop without returning — `max_turns` exhausted.
        let elapsed = started.elapsed();
        Err(self.emit_cap_and_build_err(
            TurnCapKind::Turns,
            limits.max_turns,
            &limits,
            tokens_consumed,
            elapsed.as_secs_f64(),
        )?)
    }

    /// Shared per-turn dispatch used by both [`Self::run_turn`] (legacy
    /// single-turn) and [`Self::run`] (multi-turn). Takes the full
    /// `messages` history so the multi-turn driver can accumulate
    /// across turns. Returns the outcome plus the backend-reported
    /// `tokens_used` (when available).
    fn run_one_turn(
        &mut self,
        messages: &[ChatMessage],
        original_prompt: &str,
        turn_number: Option<u32>,
    ) -> Result<(TurnOutcome, Option<u64>)> {
        let tools = self.tool_catalog();

        let response = {
            let model = self
                .loaded_model
                .as_mut()
                .ok_or(Error::NoBackendConfigured)?;
            model.infer(InferRequest {
                messages: messages.to_vec(),
                tools: tools.clone(),
            })?
        };

        let tools_considered: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
        let tool_selected = response.tool_calls.first().map(|c| c.name.clone());
        let step_uuid = self.record_reasoning_step(
            original_prompt,
            &response.reasoning,
            tools_considered,
            tool_selected,
        )?;
        let step_id = step_uuid.to_string();

        // v2 per-turn entries fire only on the multi-turn path AND only
        // when the ledger itself is v2. Both conditions must hold —
        // emitting tool_call/tool_result on a v1 ledger would taint
        // the chain with entry types v1 readers don't recognize as
        // turn-scoped.
        let emit_v2_entries =
            turn_number.is_some() && self.schema_version() == LedgerSchemaVersion::V2;

        let mut outcomes = Vec::with_capacity(response.tool_calls.len());
        for (call_index, call) in response.tool_calls.into_iter().enumerate() {
            if emit_v2_entries {
                let turn_n = turn_number.unwrap_or(0);
                let tool_call_id = format!("turn-{turn_n}-call-{call_index}");
                let origin = crate::turn::origin_of(&call.name);
                let args_hex = sha256_hex_of_json(&call.arguments);
                self.write_tool_call_entry(turn_n, &tool_call_id, &call.name, &origin, &args_hex)?;
            }
            let outcome = self.dispatch_tool_call(call, Some(&step_id))?;
            if emit_v2_entries {
                let turn_n = turn_number.unwrap_or(0);
                let tool_call_id = format!("turn-{turn_n}-call-{call_index}");
                let result_value = tool_call_result_to_value(&outcome.result);
                let result_hex = sha256_hex_of_json(&result_value);
                self.write_tool_result_entry(turn_n, &tool_call_id, &result_hex, result_value)?;
            }
            outcomes.push(outcome);
        }

        let assistant_text = response.assistant_text;
        let tokens_used = response.tokens_used;
        Ok((
            TurnOutcome {
                assistant_text,
                tool_calls: outcomes,
                reasoning_step_id: step_id,
            },
            tokens_used,
        ))
    }

    /// Write the `TurnCapExceeded` Violation entry to the ledger and
    /// build the structured [`Error::TurnCapExceeded`] the driver
    /// returns. Either the entry-write or the error build can fail —
    /// both surface to the caller, which is correct: a failed ledger
    /// write is itself a load-bearing failure on the security-review path.
    fn emit_cap_and_build_err(
        &mut self,
        bound: TurnCapKind,
        at_turn: u32,
        limits: &TurnLimits,
        tokens_consumed: u64,
        wallclock_seconds: f64,
    ) -> Result<Error> {
        self.write_turn_cap_violation(
            bound,
            at_turn,
            limits.max_turns,
            tokens_consumed,
            limits.max_tokens,
            wallclock_seconds,
            limits.max_seconds,
        )?;
        Ok(Error::TurnCapExceeded {
            bound,
            at_turn,
            max_turns: limits.max_turns,
            tokens_consumed,
            max_tokens: limits.max_tokens,
            wallclock_seconds,
            max_seconds: limits.max_seconds,
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
                arguments: call.arguments,
                result: ToolCallResult::Unroutable(format!(
                    "tool name {:?} not in <namespace>__<tool> shape",
                    call.name
                )),
                classifier_verdict: None,
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
            arguments: call.arguments,
            result,
            classifier_verdict: None,
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

/// Derive a [`ToolOrigin`] from the `<namespace>__<tool>` name the
/// model emitted. Native namespaces map to their fixed origins; the
/// rest are treated as MCP. Used by the multi-turn driver's
/// adversarial pre-filter so the classifier and the ledger see the
/// same provenance string.
fn origin_of(tool_name: &str) -> ToolOrigin {
    if let Some((ns, t)) = split_mcp_name(tool_name) {
        match ns {
            "filesystem" => ToolOrigin::Filesystem,
            "network" => ToolOrigin::NetworkOutbound,
            "exec" => ToolOrigin::Exec,
            _ => ToolOrigin::McpServer {
                server_name: ns.to_string(),
                tool_name: t.to_string(),
            },
        }
    } else {
        // Unroutable tool names (no `__` separator) are dispatched
        // as Unroutable outcomes; the classifier still runs on the
        // synthesised JSON. Fall back to a McpServer label so the
        // ledger entry carries something resolvable.
        ToolOrigin::McpServer {
            server_name: "unknown".to_string(),
            tool_name: tool_name.to_string(),
        }
    }
}

/// Serialise a [`ToolCallResult`] into JSON suitable for inclusion in
/// a [`ChatRole::Tool`] history message. Each variant becomes a
/// discriminated object — the model sees the structure and the verdict
/// without needing to parse free text.
fn tool_call_result_to_value(result: &ToolCallResult) -> serde_json::Value {
    match result {
        ToolCallResult::Success(v) => serde_json::json!({"status": "success", "value": v}),
        ToolCallResult::Denied(r) => serde_json::json!({"status": "denied", "reason": r}),
        ToolCallResult::RequiresApproval(r) => {
            serde_json::json!({"status": "requires_approval", "reason": r})
        }
        ToolCallResult::Unroutable(r) => {
            serde_json::json!({"status": "unroutable", "reason": r})
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

/// SHA-256 of the canonical serialization of a [`serde_json::Value`].
/// Used by the v2 ledger path to hash tool-call args and tool-result
/// payloads into `tool_call.requestArgsHex` / `tool_result.resultHashHex`.
///
/// `serde_json::Map` is backed by `BTreeMap` in this build (no
/// `preserve_order` feature), so `to_string` is byte-deterministic
/// across runs and across reimplementations of the verifier in other
/// languages.
fn sha256_hex_of_json(value: &serde_json::Value) -> String {
    let canonical = serde_json::to_string(value).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    hex::encode(hasher.finalize())
}

/// SHA-256 of the canonical serialization of a chat-message slice.
/// Used by the v2 ledger path to populate `turn_start.contextDigestHex`
/// so F8 replay can detect a manifest mutation or message-history
/// tampering between turns.
fn sha256_hex_of_messages(messages: &[ChatMessage]) -> String {
    let canonical = serde_json::to_string(messages).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    hex::encode(hasher.finalize())
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
