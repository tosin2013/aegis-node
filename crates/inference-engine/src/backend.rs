//! `Backend` trait + chat-time types — the runtime's only
//! abstraction over an inference engine.
//!
//! Per LLM-B / [issue #71](https://github.com/tosin2013/aegis-node/issues/71)
//! and ADR-014 §"Decision item 3". Lives in `aegis-inference-engine`
//! (not `aegis-llama-backend`) so the runtime build path doesn't drag
//! llama.cpp's C++ surface into every workspace PR. Concrete impls
//! (currently `LlamaCppBackend` in `crates/llama-backend`) take a dep
//! on this crate; the runtime takes a dep on neither.
//!
//! ## Types
//!
//! - [`ChatMessage`] / [`ChatRole`] — one chat turn in role-content
//!   shape.
//! - [`ToolDecl`] — declaration of a tool the model may call. Names
//!   are formatted as `"<server>__<tool>"` for MCP tools so the
//!   driver can split on `__` to dispatch via
//!   `Session::mediate_mcp_tool_call` (per ADR-018).
//! - [`ToolCall`] — one tool call the model emitted.
//! - [`InferRequest`] / [`InferResponse`] — the per-turn shape.
//!
//! ## Traits
//!
//! - [`Backend`] — loads a model from disk. `Send + Sync`; one per
//!   process is the typical pattern.
//! - [`LoadedModel`] — owns the loaded weights, runs one chat turn
//!   per `infer` call. `Send + !Sync` — `infer` mutates KV-cache /
//!   sampler state in place.
//!
//! ## Error type
//!
//! [`BackendError`] is opaque: a typed [`BackendErrorKind`] +
//! detail string. Concrete impls map their internal errors to a
//! kind on the way out so the runtime never has to depend on
//! llama.cpp's own error types.

use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Roles modern instruct models recognize. Vendor-specific roles
/// ("function", "developer", etc.) get represented as
/// [`ChatRole::Tool`] or by tunneling into [`ChatMessage::content`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    /// System prompt.
    System,
    /// User input.
    User,
    /// Model output.
    Assistant,
    /// Result of a tool call, returned to the model on the next turn.
    Tool,
}

impl ChatRole {
    /// Lowercase string the chat template expects.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ChatRole::System => "system",
            ChatRole::User => "user",
            ChatRole::Assistant => "assistant",
            ChatRole::Tool => "tool",
        }
    }
}

/// A single chat turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Role of the message (system / user / assistant / tool).
    pub role: ChatRole,
    /// Free-text content. Tool calls are surfaced through
    /// [`ToolCall`] in the response, not embedded here.
    pub content: String,
}

/// Declaration of a tool the model may call. Surfaces directly into
/// the OpenAI-compatible `tools` block of the chat template (which
/// every modern instruct-with-tools GGUF understands).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDecl {
    /// Tool name. For MCP tools (per ADR-018) this is formatted as
    /// `"<server_name>__<tool_name>"` so the driver can split on the
    /// first `__` and dispatch through `mediate_mcp_tool_call`. The
    /// double-underscore convention avoids collisions with dots that
    /// commonly appear in tool names (e.g., `filesystem.read`).
    pub name: String,
    /// Free-text description for the model. Pulled from the manifest's
    /// catalog; the security review reads it.
    pub description: String,
    /// JSON Schema for the tool's arguments. Opaque to this layer —
    /// the manifest validator already normalized it.
    pub arguments_schema: serde_json::Value,
}

/// One tool call the model emitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool name from the catalog. See [`ToolDecl::name`] for the
    /// `<server>__<tool>` format the driver expects for MCP dispatch.
    pub name: String,
    /// JSON-shaped arguments. The mediator interprets them per tool.
    pub arguments: serde_json::Value,
}

/// Inputs to one inference turn.
#[derive(Debug, Clone)]
pub struct InferRequest {
    /// Conversation so far, in chronological order.
    pub messages: Vec<ChatMessage>,
    /// Tool catalog the model may call this turn. Empty when no tools
    /// are available — every modern instruct model handles that
    /// gracefully.
    pub tools: Vec<ToolDecl>,
}

/// Outputs of one inference turn.
#[derive(Debug, Clone)]
pub struct InferResponse {
    /// The model's free-text reasoning (everything outside any
    /// `<tool_call>` blocks, with leading/trailing whitespace
    /// trimmed). Surfaces into F5 `ReasoningStep` ledger entries. May
    /// be empty when the model goes straight to a tool call.
    pub reasoning: String,
    /// Parsed tool calls. The runtime dispatches each through the
    /// appropriate `mediate_*` method.
    pub tool_calls: Vec<ToolCall>,
    /// Optional terminal assistant message — set when the model
    /// produced text intended for the user (vs. only tool calls).
    pub assistant_text: Option<String>,
}

/// Trait for anything that can load a model from disk and return a
/// [`LoadedModel`] handle. `Send + Sync` because production runtimes
/// share a single backend across threads.
pub trait Backend: Send + Sync {
    /// Load a GGUF (or other supported format) and return a stateful
    /// model handle.
    fn load(&self, model_path: &Path) -> Result<Box<dyn LoadedModel>, BackendError>;
}

/// One inference session against a loaded model. `Send` so the
/// runtime can hand it off across thread boundaries; **not** `Sync`
/// — `infer` mutates KV-cache state in place.
pub trait LoadedModel: Send {
    /// Run a single chat-turn inference. Mutates the model's per-call
    /// sampler state but otherwise leaves the loaded weights alone.
    fn infer(&mut self, request: InferRequest) -> Result<InferResponse, BackendError>;
}

/// Typed error from a [`Backend`] / [`LoadedModel`] call. The kind
/// is the discriminant the runtime uses to pick a violation /
/// violation reason; `detail` is a human-readable extension that
/// flows into the ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendError {
    /// Which gate refused.
    pub kind: BackendErrorKind,
    /// Free-text detail from the impl (FFI message, parser error,
    /// etc.).
    pub detail: String,
}

impl BackendError {
    /// Build a new error from a kind + detail.
    pub fn new(kind: BackendErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for BackendError {}

/// Categories of [`BackendError`]. Stable surface — concrete impls
/// map their internal errors here on the way out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendErrorKind {
    /// Model file unreadable / not found.
    ModelFileUnreadable,
    /// Model file present but rejected by the parser (not a valid
    /// GGUF, version mismatch, ...).
    ModelLoadFailed,
    /// Could not allocate / initialize an inference context.
    SessionInitFailed,
    /// Tokenization of the prompt failed.
    Tokenization,
    /// `infer` returned an error from the FFI's decode step.
    Inference,
    /// Output contained invalid UTF-8.
    InvalidUtf8,
    /// Caller supplied an impossible configuration.
    InvalidConfig,
    /// Backend was already initialized in this process.
    BackendAlreadyInitialized,
    /// Backend init failed for a system-level reason.
    BackendInitFailed,
    /// Other / impl-specific. The detail string carries the specifics.
    Other,
}

impl fmt::Display for BackendErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BackendErrorKind::ModelFileUnreadable => "model file unreadable",
            BackendErrorKind::ModelLoadFailed => "model load failed",
            BackendErrorKind::SessionInitFailed => "session init failed",
            BackendErrorKind::Tokenization => "tokenization failed",
            BackendErrorKind::Inference => "inference decode failed",
            BackendErrorKind::InvalidUtf8 => "invalid UTF-8 in output",
            BackendErrorKind::InvalidConfig => "invalid configuration",
            BackendErrorKind::BackendAlreadyInitialized => "backend already initialized",
            BackendErrorKind::BackendInitFailed => "backend init failed",
            BackendErrorKind::Other => "backend error",
        };
        f.write_str(s)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn chat_role_serializes_lowercase() {
        let m = ChatMessage {
            role: ChatRole::Assistant,
            content: "hi".to_string(),
        };
        let s = serde_json::to_string(&m).unwrap();
        assert!(s.contains(r#""role":"assistant""#), "{s}");
    }

    #[test]
    fn backend_error_display_includes_kind_and_detail() {
        let e = BackendError::new(BackendErrorKind::Tokenization, "bad bytes");
        assert_eq!(e.to_string(), "tokenization failed: bad bytes");
    }
}
