//! `LiteRtLmBackend` — the LiteRT-LM impl of [`aegis_inference_engine::Backend`].
//!
//! Per LiteRT-B / [issue #96](https://github.com/tosin2013/aegis-node/issues/96)
//! and [ADR-023](../../docs/adrs/023-litertlm-as-second-inference-backend.md)
//! §"Implementation plan" item 2. The trait + chat-time types
//! ([`InferRequest`], [`ToolCall`], etc.) live in
//! `aegis-inference-engine` so the runtime trust boundary doesn't
//! drag LiteRT-LM's C++ surface; this module is the only place that
//! bridges the two.
//!
//! Scope:
//!
//! - Format incoming [`InferRequest`] messages + tools into a flat
//!   prompt (Phase 1 — see "Conversation API future" below for the
//!   production-quality path).
//! - Run the prompt through [`Session::infer`] (LiteRT-A's wrapper).
//! - Parse `<tool_call>{...}</tool_call>` JSON blocks out of the raw
//!   output. Same convention modern instruct models emit; reused
//!   from the llama-backend so audit reasoning doesn't fork.
//!
//! Determinism (per ADR-023 §"Determinism + replay") is wired
//! through [`SessionOptions::determinism`] — the manifest's
//! `inference.determinism` block translates to those knobs and the
//! sampler param chain in [`crate::Session::new`] honors them.
//! `temperature > 0.0` is **refused at boot** with
//! [`BackendErrorKind::InvalidConfig`] until upstream's
//! `LiteRT-LM #2080` / `#2081` GPU sampler-determinism fix lands;
//! Phase 1 is CPU + greedy only.
//!
//! ## Conversation API future
//!
//! Upstream offers a higher-level Conversation API
//! (`litert_lm_conversation_send_message` with structured
//! tools_json + messages_json + constrained-decoding) that yields
//! pre-parsed tool calls. The flat-prompt path here is a placeholder:
//! it works for any text-in/text-out instruct model but doesn't
//! exploit Gemma 4's grammar-constrained tool-call decoder. Once
//! LiteRT-C ships a real Gemma 4 fixture and we have a runnable
//! smoke test, swap [`infer`] over to the Conversation API in a
//! follow-up — see the issue tracker.

use std::path::Path;

use aegis_inference_engine::{
    Backend as RuntimeBackend, BackendError, BackendErrorKind, InferRequest, InferResponse,
    LoadedModel, ToolCall, ToolDecl,
};

use crate::{Engine, LiteRtError, Session, SessionOptions};

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
        let prompt = format_chat_prompt(&request)?;

        let mut session = Session::new(&self.engine, self.options.clone()).map_err(map_err)?;
        let raw = session.infer(&prompt).map_err(map_err)?;
        Ok(parse_response(&raw))
    }
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

/// Format `request.messages` + `request.tools` into a flat prompt
/// the model can consume. Phase 1 placeholder — the production path
/// uses upstream's Conversation API (see module docstring).
///
/// Format mirrors a generic instruct-model layout:
///
/// ```text
/// <available_tools>
/// [tool catalog as JSON]
/// </available_tools>
///
/// system: <system prompt>
/// user: <user message>
/// tool: <tool result>
/// ...
/// assistant:
/// ```
///
/// Modern instruct models trained on tool-call data understand this
/// shape well enough for Phase 1 demonstrations. The Gemma 4 family
/// in particular emits structured `<tool_call>{...}</tool_call>`
/// blocks the [`parse_response`] helper below extracts.
fn format_chat_prompt(request: &InferRequest) -> Result<String, BackendError> {
    let mut out = String::new();

    if !request.tools.is_empty() {
        let tools_json = serialize_tools(&request.tools)?;
        out.push_str("<available_tools>\n");
        out.push_str(&tools_json);
        out.push_str("\n</available_tools>\n\n");
    }

    for m in &request.messages {
        out.push_str(m.role.as_str());
        out.push_str(": ");
        out.push_str(&m.content);
        out.push('\n');
    }
    out.push_str("assistant: ");
    Ok(out)
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

/// Parse the model's raw output into reasoning / tool calls /
/// assistant text. Tool calls are extracted from
/// `<tool_call>{...}</tool_call>` JSON blocks (the convention modern
/// instruct-with-tools models emit). Anything outside those blocks
/// is treated as the assistant's reasoning + final message.
///
/// Same shape as the llama-backend parser (intentionally) — once we
/// switch to the Conversation API the parser becomes redundant
/// (constrained decoding upstream produces structured tool_calls
/// directly), but the flat-prompt path needs it now.
pub(crate) fn parse_response(raw: &str) -> InferResponse {
    const OPEN: &str = "<tool_call>";
    const CLOSE: &str = "</tool_call>";

    let mut reasoning = String::new();
    let mut tool_calls = Vec::new();
    let mut cursor = 0;

    while let Some(start) = raw[cursor..].find(OPEN) {
        let abs_start = cursor + start;
        reasoning.push_str(&raw[cursor..abs_start]);

        let body_start = abs_start + OPEN.len();
        let Some(close_rel) = raw[body_start..].find(CLOSE) else {
            // Unterminated `<tool_call>` — treat the rest as plain
            // text. The next turn's request can show the model the
            // problem.
            reasoning.push_str(&raw[abs_start..]);
            cursor = raw.len();
            break;
        };
        let body_end = body_start + close_rel;
        let body = raw[body_start..body_end].trim();

        if let Some(call) = parse_tool_call_body(body) {
            tool_calls.push(call);
        } else {
            // Malformed JSON — keep the original block in reasoning
            // verbatim so downstream debugging can see what the model
            // emitted.
            reasoning.push_str(&raw[abs_start..body_end + CLOSE.len()]);
        }

        cursor = body_end + CLOSE.len();
    }
    // Trailing text after the last `</tool_call>` is reasoning.
    reasoning.push_str(&raw[cursor..]);

    let reasoning = reasoning.trim().to_string();
    let assistant_text = if reasoning.is_empty() {
        None
    } else {
        Some(reasoning.clone())
    };
    InferResponse {
        reasoning,
        tool_calls,
        assistant_text,
    }
}

/// Parse a `<tool_call>...</tool_call>` body into a [`ToolCall`].
/// Strict serde_json: the Conversation API's structured output (and
/// constrained-decoded Gemma 4 output) is well-formed JSON. If a
/// future model emits the doubled-brace Qwen quirk, surface that as
/// a separate fallback then; today's pin doesn't need it.
fn parse_tool_call_body(body: &str) -> Option<ToolCall> {
    #[derive(serde::Deserialize)]
    struct Wire {
        name: String,
        #[serde(default)]
        arguments: serde_json::Value,
    }
    let wire: Wire = serde_json::from_str(body).ok()?;
    Some(ToolCall {
        name: wire.name,
        arguments: wire.arguments,
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
        LiteRtError::EngineSettingsAllocFailed | LiteRtError::SessionConfigAllocFailed => {
            (BackendErrorKind::SessionInitFailed, e.to_string())
        }
        LiteRtError::SessionInitFailed => (BackendErrorKind::SessionInitFailed, e.to_string()),
        LiteRtError::InferenceFailed | LiteRtError::NoCandidates => {
            (BackendErrorKind::Inference, e.to_string())
        }
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
    fn parse_response_extracts_single_tool_call() {
        let raw = r#"Let me think.<tool_call>{"name":"filesystem__read","arguments":{"path":"/etc/passwd"}}</tool_call>"#;
        let resp = parse_response(raw);
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].name, "filesystem__read");
        assert_eq!(resp.tool_calls[0].arguments["path"], "/etc/passwd");
        assert_eq!(resp.reasoning, "Let me think.");
    }

    #[test]
    fn parse_response_handles_pure_text_response() {
        let raw = "The capital of France is Paris.";
        let resp = parse_response(raw);
        assert_eq!(resp.tool_calls.len(), 0);
        assert_eq!(resp.reasoning, "The capital of France is Paris.");
        assert_eq!(
            resp.assistant_text.as_deref(),
            Some("The capital of France is Paris.")
        );
    }

    #[test]
    fn parse_response_handles_multiple_tool_calls() {
        let raw = r#"<tool_call>{"name":"a","arguments":{"x":1}}</tool_call>middle<tool_call>{"name":"b","arguments":{"y":2}}</tool_call>"#;
        let resp = parse_response(raw);
        assert_eq!(resp.tool_calls.len(), 2);
        assert_eq!(resp.tool_calls[0].name, "a");
        assert_eq!(resp.tool_calls[1].name, "b");
        assert_eq!(resp.reasoning, "middle");
    }

    #[test]
    fn parse_response_unterminated_tool_call_falls_back_to_reasoning() {
        let raw = r#"text <tool_call>{"name":"oops"}"#;
        let resp = parse_response(raw);
        assert_eq!(resp.tool_calls.len(), 0);
        assert!(resp.reasoning.contains("<tool_call>"));
    }

    #[test]
    fn parse_response_malformed_json_kept_as_reasoning() {
        let raw = r#"<tool_call>not json</tool_call>"#;
        let resp = parse_response(raw);
        assert_eq!(resp.tool_calls.len(), 0);
        assert!(resp.reasoning.contains("not json"));
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
        assert!(err.detail.contains("#2080") && err.detail.contains("#2081"));
    }

    #[test]
    fn format_chat_prompt_includes_messages_and_tool_catalog() {
        let request = InferRequest {
            messages: vec![
                ChatMessage {
                    role: ChatRole::System,
                    content: "You are helpful.".to_string(),
                },
                ChatMessage {
                    role: ChatRole::User,
                    content: "Read /etc/passwd".to_string(),
                },
            ],
            tools: vec![ToolDecl {
                name: "filesystem__read".to_string(),
                description: "Read a file".to_string(),
                arguments_schema: serde_json::json!({"type": "object"}),
            }],
        };
        let prompt = format_chat_prompt(&request).unwrap();
        assert!(prompt.contains("<available_tools>"));
        assert!(prompt.contains("filesystem__read"));
        assert!(prompt.contains("system: You are helpful."));
        assert!(prompt.contains("user: Read /etc/passwd"));
        assert!(prompt.ends_with("assistant: "));
    }

    #[test]
    fn format_chat_prompt_skips_tool_block_when_no_tools() {
        let request = InferRequest {
            messages: vec![ChatMessage {
                role: ChatRole::User,
                content: "Hi".to_string(),
            }],
            tools: vec![],
        };
        let prompt = format_chat_prompt(&request).unwrap();
        assert!(!prompt.contains("<available_tools>"));
        assert!(prompt.contains("user: Hi"));
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
