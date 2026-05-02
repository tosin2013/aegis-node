//! `LiteRtLmBackend` — the LiteRT-LM impl of [`aegis_inference_engine::Backend`].
//!
//! Per LiteRT-B / [issue #96](https://github.com/tosin2013/aegis-node/issues/96),
//! LiteRT-D / [issue #119](https://github.com/tosin2013/aegis-node/issues/119),
//! and [ADR-023](../../docs/adrs/023-litertlm-as-second-inference-backend.md).
//! The trait + chat-time types ([`InferRequest`], [`ToolCall`],
//! etc.) live in `aegis-inference-engine` so the runtime trust
//! boundary doesn't drag LiteRT-LM's C++ surface; this module is
//! the only place that bridges the two.
//!
//! Scope:
//!
//! - Split [`InferRequest::messages`] into the system prompt,
//!   history messages, and the latest user message that drives the
//!   turn (see [`split_messages`]).
//! - Open a [`crate::Conversation`] — the higher-level LiteRT-LM
//!   chat surface that applies the model's bundled chat template
//!   and threads `tools_json` through the upstream constrained-
//!   decoder.
//! - Send the latest user message via
//!   `litert_lm_conversation_send_message` and parse the structured
//!   JSON response into [`InferResponse`] with reasoning + parsed
//!   tool calls.
//!
//! Pre-LiteRT-D, this module used a flat-prompt path through
//! [`crate::Session::infer`] that bypassed Gemma 4's chat template
//! entirely; the model emitted empty output. Switching to the
//! Conversation API closes that gap.
//!
//! Determinism (per ADR-023 §"Determinism + replay") is still wired
//! through [`SessionOptions::determinism`]; `temperature > 0.0` is
//! refused at boot. The actual byte-determinism guarantee is
//! upstream-blocked on LiteRT-LM #2080 (CPU sampler) — the gate
//! here documents intent and keeps cross-backend manifest
//! declaration consistent.

use std::path::Path;

use aegis_inference_engine::{
    Backend as RuntimeBackend, BackendError, BackendErrorKind, ChatRole, InferRequest,
    InferResponse, LoadedModel, ToolCall, ToolDecl,
};

use crate::{Conversation, Engine, LiteRtError, SessionOptions};

/// LiteRT-LM implementation of [`aegis_inference_engine::Backend`].
/// Wraps the LiteRT-A safe-wrapper crate and produces
/// [`LiteRtLmLoadedModel`]s on demand.
///
/// `Backend::load` enforces the Phase 1 determinism gate
/// (`temperature == 0.0` → greedy only) before calling into the FFI.
/// Failing fast at boot — rather than at the first inference turn —
/// matches the `temperature > 0.0` "errors at boot" criterion in the
/// LiteRT-B acceptance list.
#[derive(Debug, Clone)]
pub struct LiteRtLmBackend {
    options: SessionOptions,
}

impl LiteRtLmBackend {
    /// Construct a new backend handle. The `options.determinism`
    /// block is validated lazily on each [`load`] — keeping `new`
    /// total simplifies CLI wiring (the manifest's determinism
    /// knobs aren't fully resolved until session boot).
    #[must_use]
    pub fn new(options: SessionOptions) -> Self {
        Self { options }
    }
}

impl RuntimeBackend for LiteRtLmBackend {
    fn load(&self, model_path: &Path) -> Result<Box<dyn LoadedModel>, BackendError> {
        // Phase 1 determinism gate. CPU + greedy only — anything
        // that would activate the seed-aware random sampler is
        // refused until upstream's GPU sampler-determinism fix
        // lands (LiteRT-LM #2080 / PR #2081).
        check_phase1_determinism(&self.options)?;

        // Surface upstream INFO+ logs to stderr if AEGIS_LITERTLM_DEBUG
        // is set — the LiteRT-LM C ABI returns NULL on session-create
        // failure without a structured reason, and the upstream log
        // line is the only signal an operator can act on. Off by
        // default to keep the demo recordings noise-free.
        if std::env::var_os("AEGIS_LITERTLM_DEBUG").is_some() {
            crate::set_min_log_level(0);
        }

        let engine = Engine::load(model_path).map_err(map_err)?;
        Ok(Box::new(LiteRtLmLoadedModel {
            engine,
            options: self.options.clone(),
        }))
    }
}

/// LiteRT-LM implementation of [`LoadedModel`]. Owns the loaded
/// engine; opens a fresh inference [`Session`] on each `infer` call
/// so per-turn sampler state doesn't leak between turns.
pub struct LiteRtLmLoadedModel {
    engine: Engine,
    options: SessionOptions,
}

impl std::fmt::Debug for LiteRtLmLoadedModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiteRtLmLoadedModel")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

impl LoadedModel for LiteRtLmLoadedModel {
    fn infer(&mut self, request: InferRequest) -> Result<InferResponse, BackendError> {
        // Per LiteRT-D (#119): use the Conversation API so the
        // model's bundled chat template applies and the upstream
        // constrained-decoder produces structured tool calls. The
        // earlier flat-prompt path elicited empty output from
        // Gemma 4 because it bypassed Gemma's specific turn-marker
        // tokens.

        let (system_message, history_messages, latest_user) = split_messages(&request)?;
        let history_json = serialize_history(history_messages)?;
        let tools_json = if request.tools.is_empty() {
            None
        } else {
            Some(serialize_tools(&request.tools)?)
        };

        let mut conv = Conversation::open(
            &self.engine,
            self.options.clone(),
            system_message.as_deref(),
            tools_json.as_deref(),
            history_json.as_deref(),
        )
        .map_err(map_err)?;

        // The new user message goes through send_message — that's
        // the LiteRT-LM convention vs. embedding it in
        // messages_json. Wrap as a JSON object the C ABI accepts.
        let message_json = serde_json::json!({
            "role": "user",
            "content": latest_user,
        })
        .to_string();
        let response = conv.send_message(&message_json, None).map_err(map_err)?;
        let raw_json = response.as_str().map_err(map_err)?;
        parse_conversation_response(raw_json)
    }
}

/// Split [`InferRequest::messages`] into:
/// 1. the optional **system** prompt (LiteRT-LM takes this
///    separately from the conversation messages),
/// 2. **history** messages — every assistant/user/tool message
///    *except* the most recent user message,
/// 3. the **latest user message** content (sent via send_message).
///
/// Returns an [`BackendErrorKind::InvalidConfig`] if there's no
/// user message to send (an inference call with only a system
/// turn is malformed for the LiteRT-LM Conversation API).
fn split_messages(
    request: &InferRequest,
) -> Result<
    (
        Option<String>,
        &[aegis_inference_engine::ChatMessage],
        String,
    ),
    BackendError,
> {
    let mut system_message: Option<String> = None;
    let mut start_idx = 0;
    if let Some(first) = request.messages.first() {
        if matches!(first.role, ChatRole::System) {
            system_message = Some(first.content.clone());
            start_idx = 1;
        }
    }

    let body = &request.messages[start_idx..];
    let last_user_idx = body
        .iter()
        .rposition(|m| matches!(m.role, ChatRole::User))
        .ok_or_else(|| {
            BackendError::new(
                BackendErrorKind::InvalidConfig,
                "InferRequest has no user message — Conversation API needs at least one to send",
            )
        })?;

    let latest_user = body[last_user_idx].content.clone();
    let history = &body[..last_user_idx];
    Ok((system_message, history, latest_user))
}

/// Serialize history messages into the JSON-array shape LiteRT-LM's
/// Conversation API expects. Returns `None` when the history is
/// empty (the C ABI accepts a NULL `messages_json` for fresh
/// conversations).
fn serialize_history(
    history: &[aegis_inference_engine::ChatMessage],
) -> Result<Option<String>, BackendError> {
    if history.is_empty() {
        return Ok(None);
    }
    let arr: Vec<serde_json::Value> = history
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": m.role.as_str(),
                "content": m.content,
            })
        })
        .collect();
    serde_json::to_string(&arr).map(Some).map_err(|e| {
        BackendError::new(
            BackendErrorKind::Tokenization,
            format!("serialize message history: {e}"),
        )
    })
}

/// Parse the JSON response LiteRT-LM's Conversation API returns
/// into an [`InferResponse`].
///
/// Upstream's exact shape isn't documented in `c/engine.h`. Empirical
/// observation (verified inside the docker rendering container)
/// shows LiteRT-LM emits an OpenAI-compatible object:
///
/// ```json
/// {
///   "role": "assistant",
///   "content": "free-text reasoning + final message",
///   "tool_calls": [
///     {
///       "id": "call_0",
///       "type": "function",
///       "function": {
///         "name": "filesystem__read",
///         "arguments": "{\"path\": \"/data/x.txt\"}"
///       }
///     }
///   ]
/// }
/// ```
///
/// `tool_calls[].function.arguments` is a stringified JSON object
/// (per the OpenAI convention). This parser handles both stringified
/// and pre-parsed argument shapes — Gemma 4's exact format may vary
/// across upstream versions, and the parser stays tolerant.
pub(crate) fn parse_conversation_response(raw_json: &str) -> Result<InferResponse, BackendError> {
    let v: serde_json::Value = serde_json::from_str(raw_json).map_err(|e| {
        BackendError::new(
            BackendErrorKind::Inference,
            format!("conversation response is not valid JSON: {e}; raw={raw_json:?}"),
        )
    })?;

    let content = v
        .get("content")
        .and_then(|c| c.as_str())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let mut tool_calls: Vec<ToolCall> = Vec::new();
    if let Some(calls) = v.get("tool_calls").and_then(|c| c.as_array()) {
        for call in calls {
            // Two shapes accepted: OpenAI-nested (`function.name` +
            // `function.arguments`) and flat (`name` + `arguments`
            // directly on the call). Pick whichever is present.
            let (name, arguments_value) = if let Some(func) = call.get("function") {
                let name = func
                    .get("name")
                    .and_then(|n| n.as_str())
                    .ok_or_else(|| {
                        BackendError::new(
                            BackendErrorKind::Inference,
                            format!("tool_call.function missing `name`: {call}"),
                        )
                    })?
                    .to_string();
                let args = func
                    .get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                (name, args)
            } else {
                let name = call
                    .get("name")
                    .and_then(|n| n.as_str())
                    .ok_or_else(|| {
                        BackendError::new(
                            BackendErrorKind::Inference,
                            format!("tool_call missing `name`: {call}"),
                        )
                    })?
                    .to_string();
                let args = call
                    .get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                (name, args)
            };

            // OpenAI convention: `arguments` is a stringified JSON
            // object. If it's a string, re-parse; otherwise pass
            // through.
            let arguments = match arguments_value {
                serde_json::Value::String(s) => {
                    if s.is_empty() {
                        serde_json::Value::Object(Default::default())
                    } else {
                        serde_json::from_str(&s).map_err(|e| {
                            BackendError::new(
                                BackendErrorKind::Inference,
                                format!("tool_call arguments is not valid JSON: {e}; raw={s:?}"),
                            )
                        })?
                    }
                }
                other => other,
            };

            tool_calls.push(ToolCall { name, arguments });
        }
    }

    let assistant_text = if content.is_empty() {
        None
    } else {
        Some(content.clone())
    };

    Ok(InferResponse {
        reasoning: content,
        tool_calls,
        assistant_text,
    })
}

/// Refuse a [`SessionOptions`] that would activate the seed-aware
/// random sampler. Phase 1 manifests are required to declare
/// `temperature: 0.0` even though the underlying determinism
/// guarantee is broken upstream — see the **honesty note** below.
///
/// The boot-time refusal is the same posture llama-backend uses for
/// invalid configurations (typed error, never a runtime surprise).
///
/// ## Honesty note (Phase 1 caveat)
///
/// LiteRT-LM v0.10.2's CPU executor returns `UNIMPLEMENTED` for
/// every named sampler type (`kGreedy`/`kTopK`/`kTopP`) —
/// empirically `engine.cc:445 "Sampler type: N not implemented yet"`.
/// We work around this by emitting `kTypeUnspecified` (the model's
/// default baked into the `.litertlm` flatbuffer); the consequence
/// is that the manifest's `temperature=0.0` declaration is
/// **aspirational** on CPU until upstream's CPU sampler lands
/// (LiteRT-LM #2080 / PR #2081). The gate here documents intent and
/// keeps the manifest schema honest; the actual byte-determinism
/// promise depends on the upstream fix.
fn check_phase1_determinism(options: &SessionOptions) -> Result<(), BackendError> {
    let temp = options.determinism.temperature.unwrap_or(0.0);
    if temp > 0.0 {
        return Err(BackendError::new(
            BackendErrorKind::InvalidConfig,
            format!(
                "litertlm backend Phase 1 (per ADR-023) requires temperature=0.0; got temperature={temp}. \
                 Note: the CPU sampler is upstream-UNIMPLEMENTED in v0.10.2 (LiteRT-LM #2080), so the \
                 actual byte-determinism guarantee depends on that fix landing — but the gate here keeps \
                 manifest declaration consistent across backends. Set inference.determinism.temperature: 0.0 \
                 in the manifest."
            ),
        ));
    }
    Ok(())
}

/// Serialize a tool catalog into the OpenAI-compatible tools JSON
/// shape (mirroring llama-backend's serializer so audit reasoning
/// stays unified).
fn serialize_tools(tools: &[ToolDecl]) -> Result<String, BackendError> {
    let arr: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.arguments_schema,
                }
            })
        })
        .collect();
    serde_json::to_string(&arr).map_err(|e| {
        BackendError::new(
            BackendErrorKind::Tokenization,
            format!("serialize tool catalog: {e}"),
        )
    })
}

/// Map a [`LiteRtError`] into the runtime-facing
/// [`BackendError`] discriminant.
fn map_err(e: LiteRtError) -> BackendError {
    let (kind, detail) = match &e {
        LiteRtError::ModelFileUnreadable { detail, .. } => {
            (BackendErrorKind::ModelFileUnreadable, detail.clone())
        }
        LiteRtError::EngineCreationFailed { .. } => {
            (BackendErrorKind::ModelLoadFailed, e.to_string())
        }
        LiteRtError::EngineSettingsAllocFailed
        | LiteRtError::SessionConfigAllocFailed
        | LiteRtError::ConversationConfigAllocFailed => {
            (BackendErrorKind::SessionInitFailed, e.to_string())
        }
        LiteRtError::SessionInitFailed | LiteRtError::ConversationCreateFailed => {
            (BackendErrorKind::SessionInitFailed, e.to_string())
        }
        LiteRtError::InferenceFailed
        | LiteRtError::NoCandidates
        | LiteRtError::ConversationSendFailed => (BackendErrorKind::Inference, e.to_string()),
        LiteRtError::InvalidUtf8(_) => (BackendErrorKind::InvalidUtf8, e.to_string()),
        LiteRtError::InteriorNul(_) => (BackendErrorKind::InvalidConfig, e.to_string()),
        LiteRtError::InvalidConfig(s) => (BackendErrorKind::InvalidConfig, (*s).to_string()),
    };
    BackendError::new(kind, detail)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use aegis_inference_engine::{ChatMessage, ChatRole};

    #[test]
    fn parse_conversation_response_pure_text() {
        let raw = r#"{"role":"assistant","content":"The capital of France is Paris."}"#;
        let resp = parse_conversation_response(raw).unwrap();
        assert_eq!(resp.tool_calls.len(), 0);
        assert_eq!(resp.reasoning, "The capital of France is Paris.");
        assert_eq!(
            resp.assistant_text.as_deref(),
            Some("The capital of France is Paris.")
        );
    }

    #[test]
    fn parse_conversation_response_openai_nested_tool_call() {
        // OpenAI-compatible: tool_calls[].function.{name,arguments}
        // with arguments as a stringified JSON object (the upstream
        // convention LiteRT-LM follows).
        let raw = r#"{
          "role":"assistant",
          "content":"Let me read it.",
          "tool_calls":[
            {"id":"call_0","type":"function",
             "function":{"name":"filesystem__read",
                         "arguments":"{\"path\":\"/etc/passwd\"}"}}
          ]
        }"#;
        let resp = parse_conversation_response(raw).unwrap();
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].name, "filesystem__read");
        assert_eq!(resp.tool_calls[0].arguments["path"], "/etc/passwd");
        assert_eq!(resp.reasoning, "Let me read it.");
    }

    #[test]
    fn parse_conversation_response_flat_tool_call() {
        // Tolerance for a flatter shape some upstream variants emit:
        // tool_calls[].{name,arguments} directly (arguments
        // pre-parsed as an object).
        let raw = r#"{
          "content":"",
          "tool_calls":[
            {"name":"filesystem__read","arguments":{"path":"/data/x.txt"}}
          ]
        }"#;
        let resp = parse_conversation_response(raw).unwrap();
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].name, "filesystem__read");
        assert_eq!(resp.tool_calls[0].arguments["path"], "/data/x.txt");
        assert_eq!(resp.reasoning, "");
        assert_eq!(resp.assistant_text, None);
    }

    #[test]
    fn parse_conversation_response_multiple_tool_calls() {
        let raw = r#"{
          "content":"step",
          "tool_calls":[
            {"function":{"name":"a","arguments":"{}"}},
            {"function":{"name":"b","arguments":"{\"x\":1}"}}
          ]
        }"#;
        let resp = parse_conversation_response(raw).unwrap();
        assert_eq!(resp.tool_calls.len(), 2);
        assert_eq!(resp.tool_calls[0].name, "a");
        assert_eq!(resp.tool_calls[1].name, "b");
        assert_eq!(resp.tool_calls[1].arguments["x"], 1);
    }

    #[test]
    fn parse_conversation_response_invalid_json_returns_typed_error() {
        let err = parse_conversation_response("not json at all").expect_err("malformed");
        assert_eq!(err.kind, BackendErrorKind::Inference);
        assert!(err.detail.contains("not valid JSON"));
    }

    #[test]
    fn split_messages_extracts_system_history_and_latest_user() {
        let req = InferRequest {
            messages: vec![
                ChatMessage {
                    role: ChatRole::System,
                    content: "sys".to_string(),
                },
                ChatMessage {
                    role: ChatRole::User,
                    content: "u1".to_string(),
                },
                ChatMessage {
                    role: ChatRole::Assistant,
                    content: "a1".to_string(),
                },
                ChatMessage {
                    role: ChatRole::User,
                    content: "u2".to_string(),
                },
            ],
            tools: vec![],
        };
        let (system, history, latest) = split_messages(&req).unwrap();
        assert_eq!(system.as_deref(), Some("sys"));
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "u1");
        assert_eq!(history[1].content, "a1");
        assert_eq!(latest, "u2");
    }

    #[test]
    fn split_messages_refuses_request_with_no_user_turn() {
        let req = InferRequest {
            messages: vec![ChatMessage {
                role: ChatRole::System,
                content: "sys-only".to_string(),
            }],
            tools: vec![],
        };
        let err = split_messages(&req).expect_err("no user turn");
        assert_eq!(err.kind, BackendErrorKind::InvalidConfig);
    }

    #[test]
    fn check_phase1_determinism_accepts_temp_zero() {
        let mut opts = SessionOptions::default();
        opts.determinism.temperature = Some(0.0);
        opts.determinism.seed = Some(42);
        check_phase1_determinism(&opts).expect("temp=0 must be accepted");
    }

    #[test]
    fn check_phase1_determinism_accepts_temp_unset() {
        let opts = SessionOptions::default();
        check_phase1_determinism(&opts).expect("unset temp must be accepted (defaults to 0.0)");
    }

    #[test]
    fn check_phase1_determinism_refuses_warm_temperature() {
        let mut opts = SessionOptions::default();
        opts.determinism.temperature = Some(0.7);
        let err = check_phase1_determinism(&opts).expect_err("temp>0 must be refused");
        assert_eq!(err.kind, BackendErrorKind::InvalidConfig);
        assert!(err.detail.contains("temperature=0.7"));
        // Phase 1's CPU sampler is upstream-UNIMPLEMENTED (LiteRT-LM
        // #2080); when that lands the gate flips to a real
        // determinism guarantee. We just check the error message
        // names the upstream issue an operator would search for.
        assert!(err.detail.contains("#2080"), "got {err:?}");
    }

    #[test]
    fn serialize_tools_emits_openai_compatible_array() {
        let tools = vec![ToolDecl {
            name: "filesystem__read".to_string(),
            description: "Read a file".to_string(),
            arguments_schema: serde_json::json!({"type": "object"}),
        }];
        let json = serialize_tools(&tools).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed[0]["type"], "function");
        assert_eq!(parsed[0]["function"]["name"], "filesystem__read");
        assert_eq!(parsed[0]["function"]["description"], "Read a file");
    }

    #[test]
    fn map_err_invalid_config_passes_through() {
        let mapped = map_err(LiteRtError::InvalidConfig("max_tokens must be > 0"));
        assert_eq!(mapped.kind, BackendErrorKind::InvalidConfig);
    }

    #[test]
    fn map_err_inference_failure_maps_to_inference_kind() {
        let mapped = map_err(LiteRtError::InferenceFailed);
        assert_eq!(mapped.kind, BackendErrorKind::Inference);
    }

    #[test]
    fn loadedmodel_send_bound_satisfied() {
        // Compile-time check: LoadedModel must be Send so the
        // runtime can hand it across thread boundaries. If Engine
        // ever loses its `unsafe impl Send`, this test stops
        // compiling.
        fn require_send<T: Send>() {}
        require_send::<LiteRtLmLoadedModel>();
    }
}
