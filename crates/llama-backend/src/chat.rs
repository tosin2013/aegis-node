//! `LlamaCppBackend` — the llama.cpp impl of [`aegis_inference_engine::Backend`].
//!
//! Per LLM-B / [issue #71](https://github.com/tosin2013/aegis-node/issues/71)
//! and ADR-014 §"Decision item 3". The trait + chat-time types
//! ([`InferRequest`], [`ToolCall`], etc.) live in
//! `aegis-inference-engine` so the runtime trust boundary doesn't
//! drag llama.cpp's C++ surface; this module is the only place that
//! bridges the two.
//!
//! Scope:
//! - Format incoming [`InferRequest`] messages + tools through the
//!   GGUF's bundled chat template.
//! - Run the prompt through [`Session::infer`] (LLM-A's wrapper).
//! - Parse `<tool_call>{...}</tool_call>` JSON blocks out of the raw
//!   output (the convention Qwen2.5 and most modern instruct models
//!   emit).
//!
//! Out of scope here:
//! - Determinism knobs — LLM-C ([#72](https://github.com/tosin2013/aegis-node/issues/72))
//!   wires seed / temperature / top-p / top-k / repeat-penalty. Until
//!   then, sampling is greedy at temperature 0.

use std::path::Path;
use std::sync::Arc;

use aegis_inference_engine::{
    Backend as RuntimeBackend, BackendError, BackendErrorKind, InferRequest, InferResponse,
    LoadedModel, ToolCall, ToolDecl,
};
use llama_cpp_2::model::LlamaChatMessage;
use serde::Deserialize;

use crate::{Backend as LlamaBackendHandle, LlamaError, Model, Session, SessionOptions};

/// llama.cpp implementation of [`aegis_inference_engine::Backend`].
/// Wraps an LLM-A [`crate::Backend`] (the FFI handle) and produces
/// [`LlamaCppLoadedModel`]s on demand.
pub struct LlamaCppBackend {
    backend: Arc<LlamaBackendHandle>,
    options: SessionOptions,
}

impl LlamaCppBackend {
    /// Construct a new `LlamaCppBackend` around an already-initialized
    /// [`crate::Backend`]. Callers create the FFI handle once per
    /// process (see [`crate::Backend::init`]) and share it via `Arc`.
    pub fn new(backend: Arc<LlamaBackendHandle>, options: SessionOptions) -> Self {
        Self { backend, options }
    }
}

impl std::fmt::Debug for LlamaCppBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlamaCppBackend")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

impl RuntimeBackend for LlamaCppBackend {
    fn load(&self, model_path: &Path) -> Result<Box<dyn LoadedModel>, BackendError> {
        let model = Model::load(self.backend.clone(), model_path).map_err(map_err)?;
        Ok(Box::new(LlamaCppLoadedModel {
            model,
            options: self.options.clone(),
        }))
    }
}

/// llama.cpp implementation of [`LoadedModel`]. Owns the loaded
/// weights; opens a fresh inference [`Session`] on each `infer` call
/// so KV-cache state doesn't leak between turns.
pub struct LlamaCppLoadedModel {
    model: Model,
    options: SessionOptions,
}

impl std::fmt::Debug for LlamaCppLoadedModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlamaCppLoadedModel")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

impl LoadedModel for LlamaCppLoadedModel {
    fn infer(&mut self, request: InferRequest) -> Result<InferResponse, BackendError> {
        // Resolve the GGUF's bundled chat template. Models without a
        // template (rare in 2026 — most instruct GGUFs ship one) get
        // a flat fallback that just concatenates roles + content.
        let prompt = match self.model.inner_chat_template() {
            Some(tmpl) => apply_chat_template(&self.model, &tmpl, &request)?,
            None => fallback_flat_prompt(&request),
        };

        let mut session = Session::new(&self.model, self.options.clone()).map_err(map_err)?;
        let raw = session.infer(&prompt).map_err(map_err)?;
        Ok(parse_response(&raw))
    }
}

/// Format `request.messages` + `request.tools` through the GGUF's chat
/// template, asking the model to predict the assistant turn.
fn apply_chat_template(
    model: &Model,
    tmpl: &llama_cpp_2::model::LlamaChatTemplate,
    request: &InferRequest,
) -> Result<String, BackendError> {
    let messages: Vec<LlamaChatMessage> = request
        .messages
        .iter()
        .map(|m| {
            LlamaChatMessage::new(m.role.as_str().to_string(), m.content.clone()).map_err(|e| {
                BackendError::new(BackendErrorKind::Tokenization, format!("chat message: {e}"))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    if request.tools.is_empty() {
        // Plain chat-template apply — tools-disabled path.
        model
            .inner
            .apply_chat_template(tmpl, &messages, true)
            .map_err(|e| {
                BackendError::new(
                    BackendErrorKind::Tokenization,
                    format!("apply_chat_template: {e}"),
                )
            })
    } else {
        // OpenAI-compatible tools surface. We pass tools as JSON; the
        // template rendering injects them per the model's convention
        // (Qwen2.5 emits `<tools>...</tools>` in the system prompt and
        // expects responses inside `<tool_call>...</tool_call>`).
        let tools_json = serialize_tools(&request.tools)?;
        model
            .inner
            .apply_chat_template_with_tools_oaicompat(
                tmpl,
                &messages,
                Some(&tools_json),
                None,
                /* add_generation_prompt: */ true,
            )
            .map(|out| out.prompt)
            .map_err(|e| {
                BackendError::new(
                    BackendErrorKind::Tokenization,
                    format!("apply_chat_template_with_tools_oaicompat: {e}"),
                )
            })
    }
}

fn serialize_tools(tools: &[ToolDecl]) -> Result<String, BackendError> {
    // OpenAI tools shape:
    //   [{"type":"function","function":{"name":..,"description":..,"parameters":..}}]
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

/// Last-resort prompt format for a model without a chat template —
/// roles inlined, no special tokens. Most modern GGUFs ship a
/// template; this exists so a malformed model doesn't break the API.
fn fallback_flat_prompt(request: &InferRequest) -> String {
    let mut out = String::new();
    for m in &request.messages {
        out.push_str(m.role.as_str());
        out.push_str(": ");
        out.push_str(&m.content);
        out.push('\n');
    }
    out.push_str("assistant: ");
    out
}

/// Parse the model's raw output into reasoning / tool calls /
/// assistant text. Tool calls are extracted from
/// `<tool_call>{...}</tool_call>` JSON blocks (the convention Qwen2.5
/// and most modern instruct-with-tools models emit). Anything outside
/// those blocks is treated as the assistant's reasoning + final
/// message.
pub(crate) fn parse_response(raw: &str) -> InferResponse {
    const OPEN: &str = "<tool_call>";
    const CLOSE: &str = "</tool_call>";

    let mut reasoning = String::new();
    let mut tool_calls = Vec::new();
    let mut cursor = 0;

    while let Some(start) = raw[cursor..].find(OPEN) {
        let abs_start = cursor + start;
        // Anything before the `<tool_call>` is reasoning.
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
        let body = &raw[body_start..body_start + close_rel];

        if let Ok(parsed) = serde_json::from_str::<ToolCallBody>(body.trim()) {
            tool_calls.push(ToolCall {
                name: parsed.name,
                arguments: parsed.arguments.unwrap_or(serde_json::Value::Null),
            });
        } else {
            // Malformed JSON inside the block — preserve the raw
            // content as reasoning so the auditor can see what the
            // model emitted.
            reasoning.push_str(&raw[abs_start..body_start + close_rel + CLOSE.len()]);
        }

        cursor = body_start + close_rel + CLOSE.len();
    }
    reasoning.push_str(&raw[cursor..]);

    // Trim trailing whitespace from reasoning so the ledger entry is
    // stable across runs (model often emits a final newline).
    let reasoning_trimmed = reasoning.trim().to_string();

    let assistant_text = if !reasoning_trimmed.is_empty() {
        Some(reasoning_trimmed.clone())
    } else {
        None
    };

    InferResponse {
        reasoning: reasoning_trimmed,
        tool_calls,
        assistant_text,
    }
}

#[derive(Debug, Deserialize)]
struct ToolCallBody {
    name: String,
    #[serde(default)]
    arguments: Option<serde_json::Value>,
}

/// Map an LLM-A `LlamaError` to the runtime-facing `BackendError`.
fn map_err(e: LlamaError) -> BackendError {
    let (kind, detail) = match &e {
        LlamaError::BackendAlreadyInitialized => {
            (BackendErrorKind::BackendAlreadyInitialized, e.to_string())
        }
        LlamaError::BackendInitFailed(_) => (BackendErrorKind::BackendInitFailed, e.to_string()),
        LlamaError::ModelFileUnreadable { .. } => {
            (BackendErrorKind::ModelFileUnreadable, e.to_string())
        }
        LlamaError::ModelLoadFailed { .. } => (BackendErrorKind::ModelLoadFailed, e.to_string()),
        LlamaError::SessionInitFailed(_) => (BackendErrorKind::SessionInitFailed, e.to_string()),
        LlamaError::TokenizationFailed(_) => (BackendErrorKind::Tokenization, e.to_string()),
        LlamaError::InferenceFailed(_) => (BackendErrorKind::Inference, e.to_string()),
        LlamaError::InvalidUtf8(_) => (BackendErrorKind::InvalidUtf8, e.to_string()),
        LlamaError::InvalidConfig(_) => (BackendErrorKind::InvalidConfig, e.to_string()),
    };
    BackendError::new(kind, detail)
}

// --- Internal Model accessors ---------------------------------------
//
// Bridges `Model` (private about its inner llama-cpp-2 types) and
// the LLM-B impl which needs to read the chat template.

impl Model {
    /// Resolve the GGUF's bundled chat template, if any. `None` means
    /// the model didn't ship a template; callers fall back to a flat
    /// role-prefixed format. Best-effort: parser failures also return
    /// `None` rather than escalate.
    pub(crate) fn inner_chat_template(&self) -> Option<llama_cpp_2::model::LlamaChatTemplate> {
        self.inner.chat_template(None).ok()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use aegis_inference_engine::{ChatMessage, ChatRole};

    #[test]
    fn parse_response_with_no_tool_call_is_pure_reasoning() {
        let parsed = parse_response("Paris is the capital of France.");
        assert_eq!(parsed.reasoning, "Paris is the capital of France.");
        assert!(parsed.tool_calls.is_empty());
        assert_eq!(
            parsed.assistant_text.as_deref(),
            Some("Paris is the capital of France.")
        );
    }

    #[test]
    fn parse_response_extracts_single_tool_call_block() {
        let raw = r#"Looking up weather. <tool_call>{"name":"weather.get","arguments":{"city":"Paris"}}</tool_call>"#;
        let parsed = parse_response(raw);
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].name, "weather.get");
        assert_eq!(
            parsed.tool_calls[0].arguments,
            serde_json::json!({"city":"Paris"})
        );
        assert_eq!(parsed.reasoning, "Looking up weather.");
    }

    #[test]
    fn parse_response_extracts_multiple_tool_calls() {
        let raw = r#"<tool_call>{"name":"a","arguments":{}}</tool_call><tool_call>{"name":"b","arguments":{"x":1}}</tool_call>"#;
        let parsed = parse_response(raw);
        assert_eq!(parsed.tool_calls.len(), 2);
        assert_eq!(parsed.tool_calls[0].name, "a");
        assert_eq!(parsed.tool_calls[1].name, "b");
        assert_eq!(parsed.reasoning, "");
        assert_eq!(parsed.assistant_text, None);
    }

    #[test]
    fn parse_response_keeps_malformed_tool_call_in_reasoning() {
        let raw = "before <tool_call>not json</tool_call> after";
        let parsed = parse_response(raw);
        assert!(parsed.tool_calls.is_empty(), "{parsed:?}");
        assert!(
            parsed.reasoning.contains("not json"),
            "{}",
            parsed.reasoning
        );
    }

    #[test]
    fn parse_response_handles_unterminated_tool_call() {
        let raw = r#"<tool_call>{"name":"a"#;
        let parsed = parse_response(raw);
        assert!(parsed.tool_calls.is_empty());
        assert!(parsed.reasoning.contains("<tool_call>"));
    }

    #[test]
    fn fallback_flat_prompt_includes_assistant_marker() {
        let req = InferRequest {
            messages: vec![ChatMessage {
                role: ChatRole::User,
                content: "hello".to_string(),
            }],
            tools: vec![],
        };
        let prompt = fallback_flat_prompt(&req);
        assert!(prompt.contains("user: hello"), "{prompt}");
        assert!(prompt.ends_with("assistant: "), "{prompt}");
    }

    #[test]
    fn serialize_tools_matches_openai_function_shape() {
        let tools = vec![ToolDecl {
            name: "fs__read".to_string(),
            description: "read a file".to_string(),
            arguments_schema: serde_json::json!({
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"]
            }),
        }];
        let s = serialize_tools(&tools).unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v[0]["type"], "function");
        assert_eq!(v[0]["function"]["name"], "fs__read");
        assert_eq!(
            v[0]["function"]["parameters"]["properties"]["path"]["type"],
            "string"
        );
    }
}
