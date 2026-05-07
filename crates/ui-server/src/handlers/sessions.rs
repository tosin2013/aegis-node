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
) -> Response {
    if !reg.exists(&params.sid).await {
        return (
            StatusCode::NOT_FOUND,
            format!("unknown session_id {:?}", params.sid),
        )
            .into_response();
    }
    ws.on_upgrade(move |socket| handle_socket(socket, params.sid))
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

async fn handle_socket(mut socket: WebSocket, session_id: String) {
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
                if let Err(e) = handle_text_frame(&mut socket, &session_id, text).await {
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
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let frame: ClientFrame = match serde_json::from_str(&text) {
        Ok(f) => f,
        Err(e) => {
            send_error(socket, &format!("malformed frame: {e}")).await?;
            return Ok(());
        }
    };
    match frame {
        ClientFrame::UserPrompt { prompt } => run_stub_turn(socket, session_id, &prompt).await,
    }
}

/// Stub turn: emit `turn_start`, a single `assistant_text` echoing
/// the prompt, then `turn_end`. Sub-phase 1d.2b replaces this body
/// with `Session::run_turn` driving the real engine.
///
/// The intentional 50 ms delay between frames lets the SPA prove its
/// streaming-append rendering works (frames arrive separately) without
/// any backend-side complexity. Drop the sleep when the real engine
/// integration lands — token-by-token streaming gives natural pacing.
async fn run_stub_turn(
    socket: &mut WebSocket,
    _session_id: &str,
    prompt: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let turn_id = Uuid::now_v7().to_string();

    send_frame(
        socket,
        &ServerFrame::TurnStart {
            turn_id: turn_id.clone(),
        },
    )
    .await?;

    // Two chunks so the SPA's append-on-streaming path is exercised.
    tokio::time::sleep(Duration::from_millis(50)).await;
    send_frame(
        socket,
        &ServerFrame::AssistantText {
            turn_id: turn_id.clone(),
            text: format!("echo: {prompt}"),
        },
    )
    .await?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    send_frame(
        socket,
        &ServerFrame::AssistantText {
            turn_id: turn_id.clone(),
            text: "\n\n(stub backend — Session::run_turn integration ships in 1d.2b)".to_string(),
        },
    )
    .await?;

    send_frame(socket, &ServerFrame::TurnEnd { turn_id }).await?;
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
