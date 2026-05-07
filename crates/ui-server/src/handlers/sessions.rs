//! Chat-surface session management + WebSocket transport.
//!
//! Sub-phase 1d.2a per [ADR-031](../../../docs/adrs/031-community-webui-for-local-collaboration.md)
//! §"Surfaces" → "Context-aware chat" (tracker [#147](https://github.com/tosin2013/aegis-node/issues/147)).
//!
//! ## Endpoints
//!
//! | Path                        | Verb / Upgrade            | Purpose                                          |
//! |-----------------------------|---------------------------|--------------------------------------------------|
//! | `/api/v1/sessions`          | `POST`                    | Mint a new chat session, return its `session_id`. |
//! | `/api/v1/stream?sid=<id>`   | `GET` → `WebSocket`       | Bidirectional frames for one session's lifetime. |
//!
//! ## Frame protocol (v1)
//!
//! All frames are JSON text with `schema: "v1"` at the top level so
//! later sub-phases (1d.2b/c/d) can extend the type union without
//! breaking older clients. Type-tagged via `serde(tag = "type")`.
//!
//! ### Client → server
//!
//! ```json
//! {"schema":"v1","type":"user_prompt","prompt":"hello"}
//! ```
//!
//! ### Server → client
//!
//! ```json
//! {"schema":"v1","type":"turn_start","turn_id":"…"}
//! {"schema":"v1","type":"assistant_text","turn_id":"…","text":"echo: hello"}
//! {"schema":"v1","type":"turn_end","turn_id":"…"}
//! ```
//!
//! Plus `error` for protocol / runtime failures the SPA renders as a
//! red banner without dropping the connection.
//!
//! ## What 1d.2a does NOT do
//!
//! - **Real `Session::run_turn`** — that lands in 1d.2b. The current
//!   handler echoes the prompt back as `assistant_text`. The wire
//!   shape is stable; engine integration just replaces the echo body.
//! - **Tool-call frames** — reserved in the protocol enum but not
//!   emitted yet. Land in 1d.2c with the verifiable badge + tool-call
//!   card UI surfaces.
//! - **Reconnect** — the SPA closes/reopens the WS on visibility-
//!   change. Resume from a partial turn is a 1d.2b concern.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::stream::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Wire-protocol schema version. Bumped only on a breaking change to
/// the frame shape; additive type variants don't change this.
pub const SCHEMA_VERSION: &str = "v1";

/// In-memory session registry. Sub-phase 1d.2a keeps sessions
/// transient — created on `POST /api/v1/sessions`, dropped when the
/// process exits. 1d.2b will tie sessions to `Session::run_turn`'s
/// state; persistence-across-restart is a v1.0.0 multi-turn concern.
#[derive(Default, Clone)]
pub struct SessionRegistry {
    inner: Arc<RwLock<HashMap<String, SessionRecord>>>,
}

#[derive(Debug, Clone)]
struct SessionRecord {
    /// UUIDv7 string (sortable by creation). Same value as the map key.
    id: String,
    /// RFC3339 creation timestamp, surfaced in the create-response so
    /// the SPA can show "session started at" without separate API.
    created_at: String,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    async fn create(&self) -> SessionRecord {
        let id = Uuid::now_v7().to_string();
        let record = SessionRecord {
            id: id.clone(),
            created_at: rfc3339_now(),
        };
        self.inner.write().await.insert(id, record.clone());
        record
    }

    async fn exists(&self, id: &str) -> bool {
        self.inner.read().await.contains_key(id)
    }
}

/// Response for `POST /api/v1/sessions`.
#[derive(Debug, Serialize)]
pub struct SessionCreated {
    pub session_id: String,
    pub created_at: String,
    pub schema: &'static str,
}

/// `POST /api/v1/sessions` — mint a new chat session.
pub async fn create_session(State(reg): State<SessionRegistry>) -> Json<SessionCreated> {
    let rec = reg.create().await;
    tracing::info!(target: "aegis_ui_server", session_id = %rec.id, "session created");
    Json(SessionCreated {
        session_id: rec.id,
        created_at: rec.created_at,
        schema: SCHEMA_VERSION,
    })
}

/// Query params accepted on the WebSocket upgrade. The SPA sends
/// `?sid=<session_id>` so the server can reject upgrades for unknown
/// sessions before any frames flow.
#[derive(Debug, Deserialize)]
pub struct ConnectParams {
    pub sid: String,
}

/// `GET /api/v1/stream?sid=<id>` — upgrade to WebSocket and run the
/// chat loop. Rejects unknown session IDs with 404 *before* upgrade,
/// so the SPA's connection failure is unambiguous.
pub async fn stream(
    ws: WebSocketUpgrade,
    Query(params): Query<ConnectParams>,
    State(reg): State<SessionRegistry>,
    State(backend): State<std::sync::Arc<dyn crate::ChatBackend>>,
) -> Response {
    if !reg.exists(&params.sid).await {
        return (
            StatusCode::NOT_FOUND,
            format!("unknown session_id {:?}", params.sid),
        )
            .into_response();
    }
    ws.on_upgrade(move |socket| handle_socket(socket, params.sid, backend))
}

/// Server → client frame body. The wire form has `schema: "v1"` at
/// the top level (set by [`encode`]) and `type` next to whichever
/// fields the variant carries.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerFrame {
    /// Begins a turn. The client uses `turn_id` to scope the assistant_text
    /// chunks that follow.
    TurnStart { turn_id: String },
    /// Streaming assistant-output chunk. May arrive multiple times per
    /// turn; the client appends each `text` to the active turn's buffer.
    AssistantText { turn_id: String, text: String },
    /// One model-emitted tool call about to be dispatched. The
    /// `tool_call_id` pairs this frame with the subsequent
    /// `tool_result` so the SPA renders both into the same inline
    /// card. `args` is the JSON the model produced; the
    /// engine's mediators have already validated it against the
    /// manifest's allowlist by the time this frame fires.
    ToolCall {
        turn_id: String,
        tool_call_id: String,
        name: String,
        args: serde_json::Value,
    },
    /// The mediator's terminal decision for one tool call.
    /// `status` is one of `success` / `denied` / `requires_approval` /
    /// `unroutable`; `result` carries the tool's value on success or
    /// the human-readable reason on the other three.
    ToolResult {
        turn_id: String,
        tool_call_id: String,
        status: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Turn complete. No more frames will arrive for this `turn_id`
    /// until a fresh `user_prompt` triggers another turn.
    TurnEnd { turn_id: String },
    /// Protocol or runtime error. Non-fatal — the connection stays
    /// open so the operator can retry.
    Error { message: String },
}

/// Client → server frame body. Mirrors [`ServerFrame`]; same wire
/// envelope (`schema` + `type`).
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientFrame {
    /// User-typed prompt. Triggers one turn (which streams
    /// `turn_start` → `assistant_text`* → `turn_end`).
    UserPrompt { prompt: String },
}

/// Encode a [`ServerFrame`] into the wire JSON, prepending the
/// `schema: "v1"` field. Returns the JSON string; the caller wraps
/// it in a WebSocket text frame.
fn encode(frame: &ServerFrame) -> Result<String, serde_json::Error> {
    // Two-step serialise so the schema field lands at the top of the
    // JSON object alongside `type`. Equivalent to a struct with
    // `#[serde(flatten)]` but keeps the variant enum simple.
    let mut value = serde_json::to_value(frame)?;
    if let serde_json::Value::Object(map) = &mut value {
        // Insert at the front by rebuilding so JSON readers see
        // `schema` before `type`. Order isn't significant per JSON
        // spec, but a stable order helps log diffs.
        let mut ordered = serde_json::Map::with_capacity(map.len() + 1);
        ordered.insert("schema".to_string(), SCHEMA_VERSION.into());
        for (k, v) in map.iter() {
            ordered.insert(k.clone(), v.clone());
        }
        value = serde_json::Value::Object(ordered);
    }
    serde_json::to_string(&value)
}

async fn handle_socket(
    mut socket: WebSocket,
    session_id: String,
    backend: std::sync::Arc<dyn crate::ChatBackend>,
) {
    tracing::info!(target: "aegis_ui_server", session_id = %session_id, "ws connected");
    while let Some(msg) = socket.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(target: "aegis_ui_server", session_id = %session_id, err = %e, "ws recv error");
                break;
            }
        };
        match msg {
            Message::Text(text) => {
                if let Err(e) = handle_text_frame(&mut socket, &session_id, text, &backend).await {
                    tracing::warn!(target: "aegis_ui_server", session_id = %session_id, err = %e, "frame handler errored");
                }
            }
            Message::Close(_) => break,
            Message::Ping(_) | Message::Pong(_) | Message::Binary(_) => {
                // Ping/pong is handled by axum's underlying tungstenite
                // automatically. Binary frames aren't part of the v1
                // protocol; silently ignore.
            }
        }
    }
    tracing::info!(target: "aegis_ui_server", session_id = %session_id, "ws closed");
}

async fn handle_text_frame(
    socket: &mut WebSocket,
    session_id: &str,
    text: String,
    backend: &std::sync::Arc<dyn crate::ChatBackend>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let frame: ClientFrame = match serde_json::from_str(&text) {
        Ok(f) => f,
        Err(e) => {
            send_error(socket, &format!("malformed frame: {e}")).await?;
            return Ok(());
        }
    };
    match frame {
        ClientFrame::UserPrompt { prompt } => run_turn(socket, session_id, &prompt, backend).await,
    }
}

/// Drive one chat turn against the configured [`ChatBackend`]. Emits
/// `turn_start`, then char-chunked `assistant_text` frames so the
/// SPA's streaming-append rendering animates even though the
/// backend currently returns the response whole. Tool-call summaries
/// (1d.2b) trail as plain text; structured tool-call frames land in
/// 1d.2c per ADR-031.
///
/// The synchronous `ChatBackend::run_turn` runs on a `spawn_blocking`
/// thread so the inference step doesn't stall the async runtime —
/// llama.cpp / LiteRT-LM tokens are CPU-bound and would otherwise
/// block other WebSocket connections sharing the executor.
async fn run_turn(
    socket: &mut WebSocket,
    _session_id: &str,
    prompt: &str,
    backend: &std::sync::Arc<dyn crate::ChatBackend>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let turn_id = Uuid::now_v7().to_string();

    send_frame(
        socket,
        &ServerFrame::TurnStart {
            turn_id: turn_id.clone(),
        },
    )
    .await?;

    // Hand the inference call to a blocking thread. `Backend::run_turn`
    // is sync (Session::run_turn is sync); blocking the executor would
    // freeze every other WebSocket on this process.
    let prompt_owned = prompt.to_string();
    let backend_for_blocking = backend.clone();
    let result = tokio::task::spawn_blocking(move || backend_for_blocking.run_turn(&prompt_owned))
        .await
        .map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()))?;

    let outcome = match result {
        Ok(o) => o,
        Err(e) => {
            send_error(socket, &format!("backend error: {e}")).await?;
            send_frame(socket, &ServerFrame::TurnEnd { turn_id }).await?;
            return Ok(());
        }
    };

    let mut emitted_anything = false;
    if let Some(text) = outcome.assistant_text.as_ref() {
        if !text.is_empty() {
            stream_chunked(socket, &turn_id, text).await?;
            emitted_anything = true;
        }
    }

    // Structured tool-call frames per ADR-031 §"Inline tool-call cards"
    // (sub-phase 1d.2c). One `tool_call` frame announces the call;
    // one `tool_result` frame carries the mediator's terminal
    // decision. The `tool_call_id` pairs them so the SPA renders
    // both into the same inline card. Sub-phase 1d.2b flattened
    // these into plain-text summaries; the structured form lets
    // the SPA render the gate decision (success / denied /
    // requires_approval / unroutable) as a colored status pill on
    // the card.
    for call in &outcome.tool_calls {
        send_frame(
            socket,
            &ServerFrame::ToolCall {
                turn_id: turn_id.clone(),
                tool_call_id: call.call_id.clone(),
                name: call.name.clone(),
                args: call.args.clone(),
            },
        )
        .await?;
        // A small delay between announce + result so the SPA can
        // animate the card's status pill flipping from "pending" to
        // the terminal state. Negligible cost; gives the rendering
        // visible kinetic feedback.
        tokio::time::sleep(Duration::from_millis(CHUNK_DELAY_MS)).await;

        let (status, value, reason) = match &call.status {
            crate::chat::TurnToolCallStatus::Success { value } => {
                ("success", Some(value.clone()), None)
            }
            crate::chat::TurnToolCallStatus::Denied { reason } => {
                ("denied", None, Some(reason.clone()))
            }
            crate::chat::TurnToolCallStatus::RequiresApproval { reason } => {
                ("requires_approval", None, Some(reason.clone()))
            }
            crate::chat::TurnToolCallStatus::Unroutable { reason } => {
                ("unroutable", None, Some(reason.clone()))
            }
        };
        send_frame(
            socket,
            &ServerFrame::ToolResult {
                turn_id: turn_id.clone(),
                tool_call_id: call.call_id.clone(),
                status,
                value,
                reason,
            },
        )
        .await?;
        emitted_anything = true;
    }

    if !emitted_anything {
        // Either no text + no tool calls, or text was empty. The SPA
        // still needs *something* to render so the assistant bubble
        // doesn't appear empty.
        send_frame(
            socket,
            &ServerFrame::AssistantText {
                turn_id: turn_id.clone(),
                text: "(no output from backend)".to_string(),
            },
        )
        .await?;
    }

    send_frame(socket, &ServerFrame::TurnEnd { turn_id }).await?;
    Ok(())
}

/// Per-frame char budget for chunked streaming. ~80 chars per frame
/// gives the SPA enough granularity to show the typing animation
/// without flooding the WebSocket.
const CHUNK_SIZE_CHARS: usize = 80;
/// Delay between chunks in chunked streaming. 30 ms feels like
/// natural typing pace; lower values make the animation feel
/// jittery on slow connections.
const CHUNK_DELAY_MS: u64 = 30;

/// Split `text` into UTF-8-safe chunks of roughly [`CHUNK_SIZE_CHARS`]
/// characters and emit each as its own `assistant_text` frame with a
/// small delay. Once the inference backends grow real token-by-token
/// streaming, this body gets replaced with a token stream consumer
/// (the wire shape stays identical).
async fn stream_chunked(
    socket: &mut WebSocket,
    turn_id: &str,
    text: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut buf = String::with_capacity(CHUNK_SIZE_CHARS + 4);
    for ch in text.chars() {
        buf.push(ch);
        if buf.chars().count() >= CHUNK_SIZE_CHARS {
            send_frame(
                socket,
                &ServerFrame::AssistantText {
                    turn_id: turn_id.to_string(),
                    text: std::mem::take(&mut buf),
                },
            )
            .await?;
            tokio::time::sleep(Duration::from_millis(CHUNK_DELAY_MS)).await;
        }
    }
    if !buf.is_empty() {
        send_frame(
            socket,
            &ServerFrame::AssistantText {
                turn_id: turn_id.to_string(),
                text: buf,
            },
        )
        .await?;
    }
    Ok(())
}

async fn send_frame(
    socket: &mut WebSocket,
    frame: &ServerFrame,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let text = encode(frame)?;
    socket.send(Message::Text(text)).await?;
    Ok(())
}

async fn send_error(
    socket: &mut WebSocket,
    message: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    send_frame(
        socket,
        &ServerFrame::Error {
            message: message.to_string(),
        },
    )
    .await
}

/// Pure-stdlib RFC3339 timestamp helper. Mirrors the same helper in
/// `crates/cli/src/pull.rs` and `crates/ui-server/src/handlers/models.rs`
/// so this crate stays free of `chrono`. Acceptable duplication for
/// one format string.
fn rfc3339_now() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs() as i64;
    let nanos = dur.subsec_nanos();
    naive_rfc3339_from_unix(secs, nanos)
}

fn naive_rfc3339_from_unix(secs: i64, nanos: u32) -> String {
    const SECONDS_PER_DAY: i64 = 86_400;
    let days = secs.div_euclid(SECONDS_PER_DAY);
    let time_of_day = secs.rem_euclid(SECONDS_PER_DAY);
    let hh = time_of_day / 3600;
    let mm = (time_of_day % 3600) / 60;
    let ss = time_of_day % 60;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.{millis:03}Z",
        millis = nanos / 1_000_000,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn server_frame_serialises_with_schema_v1() {
        // Field order in the wire JSON isn't semantically significant
        // (JSON spec); serde_json's default Map orders alphabetically.
        // Assert against the parsed shape, not the literal string.
        let f = ServerFrame::TurnStart {
            turn_id: "abc".to_string(),
        };
        let s = encode(&f).unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["schema"], "v1");
        assert_eq!(v["type"], "turn_start");
        assert_eq!(v["turn_id"], "abc");
    }

    #[test]
    fn assistant_text_carries_text_and_turn() {
        let f = ServerFrame::AssistantText {
            turn_id: "t-1".to_string(),
            text: "hello".to_string(),
        };
        let s = encode(&f).unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["schema"], "v1");
        assert_eq!(v["type"], "assistant_text");
        assert_eq!(v["text"], "hello");
        assert_eq!(v["turn_id"], "t-1");
    }

    #[test]
    fn tool_call_frame_carries_name_and_args() {
        let f = ServerFrame::ToolCall {
            turn_id: "t-1".to_string(),
            tool_call_id: "call-0".to_string(),
            name: "filesystem__read".to_string(),
            args: serde_json::json!({"path": "/etc/hosts"}),
        };
        let s = encode(&f).unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["schema"], "v1");
        assert_eq!(v["type"], "tool_call");
        assert_eq!(v["turn_id"], "t-1");
        assert_eq!(v["tool_call_id"], "call-0");
        assert_eq!(v["name"], "filesystem__read");
        assert_eq!(v["args"]["path"], "/etc/hosts");
    }

    #[test]
    fn tool_result_success_skips_reason() {
        let f = ServerFrame::ToolResult {
            turn_id: "t-1".to_string(),
            tool_call_id: "call-0".to_string(),
            status: "success",
            value: Some(serde_json::json!({"bytes_read": 42})),
            reason: None,
        };
        let s = encode(&f).unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["status"], "success");
        assert_eq!(v["value"]["bytes_read"], 42);
        assert!(v.get("reason").is_none(), "reason must be omitted when None");
    }

    #[test]
    fn tool_result_denied_skips_value() {
        let f = ServerFrame::ToolResult {
            turn_id: "t-1".to_string(),
            tool_call_id: "call-0".to_string(),
            status: "denied",
            value: None,
            reason: Some("path /etc not in allowlist".to_string()),
        };
        let s = encode(&f).unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["status"], "denied");
        assert_eq!(v["reason"], "path /etc not in allowlist");
        assert!(v.get("value").is_none(), "value must be omitted when None");
    }

    #[test]
    fn client_user_prompt_decodes() {
        let raw = r#"{"schema":"v1","type":"user_prompt","prompt":"hi"}"#;
        let f: ClientFrame = serde_json::from_str(raw).unwrap();
        match f {
            ClientFrame::UserPrompt { prompt } => assert_eq!(prompt, "hi"),
        }
    }

    #[test]
    fn client_unknown_type_rejected() {
        let raw = r#"{"schema":"v1","type":"nonsense","prompt":"hi"}"#;
        assert!(serde_json::from_str::<ClientFrame>(raw).is_err());
    }

    #[tokio::test]
    async fn registry_create_then_exists() {
        let reg = SessionRegistry::new();
        let rec = reg.create().await;
        assert!(reg.exists(&rec.id).await);
        assert!(!reg.exists("does-not-exist").await);
    }

    #[tokio::test]
    async fn registry_ids_are_unique_uuids() {
        let reg = SessionRegistry::new();
        let a = reg.create().await;
        let b = reg.create().await;
        assert_ne!(a.id, b.id);
        // UUIDv7 → 36-char dashed form.
        assert_eq!(a.id.len(), 36);
    }
}
