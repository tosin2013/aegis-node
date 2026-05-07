//! Chat backend abstraction — the seam between the WebSocket
//! transport (in `handlers/sessions.rs`) and the inference engine
//! (in `aegis-cli` / `aegis-inference-engine`).
//!
//! Sub-phase 1d.2b decouples the two so `crates/ui-server` stays
//! free of `aegis-inference-engine` (which would pull in the
//! llama.cpp / LiteRT-LM C++ build trees, balloon compile times,
//! and tangle the feature-flag matrix). Instead the trait below
//! is the contract: `aegis-cli` provides an implementation that
//! wraps a real `Session` when booted with `--manifest` + `--model`,
//! or the [`StubBackend`] default for operators who want the chat
//! UI without a model attached.
//!
//! ## Why a trait, not a concrete type
//!
//! - **Crate-graph tidiness**: ui-server depends on `dirs`,
//!   `axum`, `serde_json`. It MUST NOT pull in
//!   `aegis-inference-engine` — that crate's dep graph touches
//!   `aegis-llama-backend` (cmake + bindgen + llama.cpp) and
//!   `aegis-litertlm-backend` (Bazel-prebuilt `.so` from OCI).
//!   A direct dep would force every workspace build path to pay
//!   that cost.
//! - **Test substitutability**: a [`MockBackend`] (in tests) lets
//!   us exercise the WS handler's frame protocol against a known
//!   response without booting a real model.
//!
//! ## Threading model
//!
//! `ChatBackend::run_turn` is **synchronous** because the
//! underlying `Session::run_turn` is. The WebSocket handler
//! invokes it via `tokio::task::spawn_blocking` so the inference
//! step doesn't stall the executor. Callers don't need to be
//! `async`; they MUST be `Send + Sync` so the trait object can
//! live behind an `Arc` shared across connections.

use std::fmt;

use serde_json::Value;

/// Result of one chat turn — what the trait impl returns to the
/// WebSocket handler. Strips the inference-engine types so this
/// crate stays leaf-level on the dep graph.
#[derive(Debug, Clone)]
pub struct TurnResult {
    /// Assistant text the model produced. `None` when the model
    /// emitted only tool calls (the chat surface still needs to
    /// render *something*; the WS handler synthesizes a placeholder
    /// in that case).
    pub assistant_text: Option<String>,
    /// Per-tool-call structured outcome, in emission order. The WS
    /// handler emits one `tool_call` + `tool_result` frame pair per
    /// entry per ADR-031 §"Inline tool-call cards." Sub-phase 1d.2b
    /// flattened this into plain-text summaries; 1d.2c switches to
    /// structured frames so the SPA renders gate decisions inline.
    pub tool_calls: Vec<TurnToolCall>,
}

/// One model-emitted tool call + its mediator outcome, structured
/// for the SPA's inline tool-call cards. The fields mirror what the
/// inference engine's `ToolCallOutcome` carries plus a stable
/// `call_id` the SPA uses to scope the `tool_call` → `tool_result`
/// frame pair.
#[derive(Debug, Clone)]
pub struct TurnToolCall {
    /// Unique-within-turn call id (UUIDv7 string). The WS handler
    /// pairs the `tool_call` and `tool_result` frames it emits via
    /// this id so the SPA knows which card to update when the
    /// result lands.
    pub call_id: String,
    /// Tool name as the model emitted it
    /// (`<namespace>__<tool>`, e.g. `filesystem__read`).
    pub name: String,
    /// Args the model passed. JSON for SPA-side rendering; the
    /// engine has already validated them against the manifest's
    /// allowlist + ADR-024 `pre_validate` clauses by the time this
    /// surfaces here.
    pub args: Value,
    /// Mediator outcome — what F2 / F3 / F6 / F7 decided.
    pub status: TurnToolCallStatus,
}

/// The four terminal mediator outcomes per
/// `aegis_inference_engine::ToolCallResult`. Encoded as an
/// externally-tagged enum so the wire JSON ends up with
/// `{"status":"success", "value": …}` style records that the SPA's
/// discriminated union consumes directly.
#[derive(Debug, Clone)]
pub enum TurnToolCallStatus {
    /// Mediator allowed and the upstream tool returned a value.
    Success { value: Value },
    /// Mediator denied — `reason` is the policy / runtime reason
    /// already in the F9 ledger as a Violation entry.
    Denied { reason: String },
    /// Mediator demanded F3 approval and the call short-circuited.
    RequiresApproval { reason: String },
    /// Tool call wasn't routable (malformed name, malformed args,
    /// unknown native namespace tool).
    Unroutable { reason: String },
}

impl TurnToolCallStatus {
    /// Lowercase-snake-case discriminator used in the wire JSON.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Success { .. } => "success",
            Self::Denied { .. } => "denied",
            Self::RequiresApproval { .. } => "requires_approval",
            Self::Unroutable { .. } => "unroutable",
        }
    }
}

/// Failure mode for a chat turn. Plain message — the WS handler
/// surfaces it as an `error` frame the SPA renders inline without
/// dropping the connection.
#[derive(Debug, Clone)]
pub struct ChatBackendError {
    pub message: String,
}

impl fmt::Display for ChatBackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ChatBackendError {}

impl ChatBackendError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// The chat surface's backend boundary. Implementations:
///
/// - [`StubBackend`] — built into this crate; returns an "operator
///   hint" message explaining how to enable real inference.
/// - `aegis-cli`'s `SessionBackend` — wraps an `Arc<Mutex<Session>>`,
///   plumbed in when `aegis ui --manifest <m> --model <m>` is set.
///
/// `Send + Sync` because the trait object lives behind an `Arc`
/// shared across WebSocket handlers.
pub trait ChatBackend: Send + Sync {
    /// Run one user-prompt turn. Synchronous on purpose — the
    /// caller wraps in `tokio::task::spawn_blocking` to keep the
    /// async runtime free.
    fn run_turn(&self, prompt: &str) -> Result<TurnResult, ChatBackendError>;
}

/// Default backend when `aegis ui` is started without
/// `--manifest`/`--model`. Echoes the prompt back with an operator
/// hint so the chat surface remains visibly functional during demos
/// — the wow-factor framing in ADR-031 expects a usable UI even
/// when no model is loaded.
pub struct StubBackend;

impl ChatBackend for StubBackend {
    fn run_turn(&self, prompt: &str) -> Result<TurnResult, ChatBackendError> {
        Ok(TurnResult {
            assistant_text: Some(format!(
                "echo: {prompt}\n\n(stub backend — start `aegis ui --manifest <m> --model <m> [--backend llama|litertlm]` to attach a real Session::run_turn)"
            )),
            tool_calls: Vec::new(),
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A test-only backend that records prompts it received + returns a
    /// caller-supplied response. Used in handler tests to exercise the
    /// WS frame protocol without booting an inference engine.
    pub struct MockBackend {
        pub response: String,
        pub calls: AtomicUsize,
    }

    impl MockBackend {
        pub fn new(response: impl Into<String>) -> Arc<Self> {
            Arc::new(Self {
                response: response.into(),
                calls: AtomicUsize::new(0),
            })
        }
    }

    impl ChatBackend for MockBackend {
        fn run_turn(&self, _prompt: &str) -> Result<TurnResult, ChatBackendError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(TurnResult {
                assistant_text: Some(self.response.clone()),
                tool_calls: Vec::new(),
            })
        }
    }

    #[test]
    fn stub_echoes_prompt_with_hint() {
        let r = StubBackend.run_turn("hi").unwrap();
        let text = r.assistant_text.unwrap();
        assert!(text.contains("echo: hi"));
        assert!(text.contains("--manifest"));
        assert!(text.contains("--model"));
        assert!(r.tool_calls.is_empty());
    }

    #[test]
    fn turn_tool_call_status_kind_round_trips_to_wire() {
        assert_eq!(
            TurnToolCallStatus::Success {
                value: serde_json::json!({"ok": true})
            }
            .kind(),
            "success",
        );
        assert_eq!(
            TurnToolCallStatus::Denied { reason: "x".into() }.kind(),
            "denied",
        );
        assert_eq!(
            TurnToolCallStatus::RequiresApproval { reason: "y".into() }.kind(),
            "requires_approval",
        );
        assert_eq!(
            TurnToolCallStatus::Unroutable { reason: "z".into() }.kind(),
            "unroutable",
        );
    }

    #[test]
    fn mock_records_call_count() {
        let m = MockBackend::new("ok");
        m.run_turn("x").unwrap();
        m.run_turn("y").unwrap();
        assert_eq!(m.calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn error_renders_via_display() {
        let e = ChatBackendError::new("boom");
        assert_eq!(format!("{e}"), "boom");
    }
}
