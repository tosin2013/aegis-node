//! Localhost web UI approval channel (F3, issue #35).
//!
//! Spins up a tiny HTTP server bound to a loopback address only. A
//! single one-shot bearer token authenticates every request — the
//! agent prints the token + URL on startup and an operator pastes
//! either into a browser or a shell client. The token is bound to the
//! channel instance and discarded on `Drop`, so once the session ends
//! it's useless.
//!
//! ## Why no TLS
//! Loopback-only. TLS adds CA / cert provisioning friction with no
//! benefit on the same machine. Cross-host approvals are the F3 mTLS
//! channel's job (issue #36).
//!
//! ## Why bearer in a header (not a cookie)
//! A drive-by browser tab on the same machine could mount a CSRF on a
//! cookie-authenticated endpoint. Requiring the token in
//! `Authorization: Bearer ...` defeats that — browsers don't send
//! arbitrary headers cross-origin without a preflight, and our server
//! doesn't honor preflights.
//!
//! ## Endpoints
//!
//! - `GET  /approvals`              — list outstanding approvals
//! - `POST /approvals/<id>/grant`   — grant; optional JSON body `{"approver": "alice"}`
//! - `POST /approvals/<id>/reject`  — reject; optional JSON body `{"reason": "..."}`
//!
//! Anything else returns 404. Any unauthenticated request returns 401.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Instant;

use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use tiny_http::{Header, Method, Request, Response, Server};
use uuid::Uuid;

use crate::{ApprovalChannel, ApprovalOutcome, ApprovalRequest, Error, Result};

/// Web channel construction. Owns the HTTP server thread; on drop it
/// signals shutdown and joins.
pub struct WebApprovalChannel {
    bound: SocketAddr,
    token: String,
    state: Arc<State>,
    server: Arc<Server>,
    server_thread: Option<thread::JoinHandle<()>>,
}

impl std::fmt::Debug for WebApprovalChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebApprovalChannel")
            .field("bound", &self.bound)
            .finish_non_exhaustive()
    }
}

#[derive(Default)]
struct State {
    pending: Mutex<HashMap<String, PendingRequest>>,
    cv: Condvar,
}

struct PendingRequest {
    request: ApprovalRequest,
    decision: Option<RecordedDecision>,
}

#[derive(Clone)]
enum RecordedDecision {
    Granted {
        approver: String,
        decided_at: DateTime<Utc>,
    },
    Rejected {
        reason: String,
        decided_at: DateTime<Utc>,
    },
}

#[derive(Serialize)]
struct ListEntry<'a> {
    request_id: &'a str,
    action_summary: &'a str,
    resource_uri: &'a str,
    access_type: &'a str,
    session_id: &'a str,
    reasoning_step_id: Option<&'a str>,
}

#[derive(Default, Deserialize)]
struct GrantBody {
    #[serde(default)]
    approver: Option<String>,
}

#[derive(Default, Deserialize)]
struct RejectBody {
    #[serde(default)]
    reason: Option<String>,
}

impl WebApprovalChannel {
    /// Bind to `bind_addr` (must resolve to a loopback IP) and spawn
    /// the HTTP worker. Returns the channel + the bound address +
    /// the printed bearer token.
    pub fn new(bind_addr: &str) -> Result<Self> {
        let parsed: SocketAddr = bind_addr
            .parse()
            .map_err(|e| Error::Channel(format!("parse bind_addr {bind_addr:?}: {e}")))?;
        if !is_loopback(parsed.ip()) {
            return Err(Error::Channel(format!(
                "web channel refuses non-loopback bind {parsed}; use 127.0.0.1 or ::1"
            )));
        }

        let server =
            Server::http(parsed).map_err(|e| Error::Channel(format!("bind {parsed}: {e}")))?;
        let bound = match server.server_addr() {
            tiny_http::ListenAddr::IP(addr) => addr,
            _ => return Err(Error::Channel("non-IP listener".to_string())),
        };
        let server = Arc::new(server);
        let state = Arc::new(State::default());
        let token = generate_token();

        let thread_state = Arc::clone(&state);
        let thread_token = token.clone();
        let thread_server = Arc::clone(&server);
        let server_thread = thread::Builder::new()
            .name("aegis-web-approval".to_string())
            .spawn(move || serve(thread_server, thread_state, thread_token))
            .map_err(Error::Io)?;

        Ok(Self {
            bound,
            token,
            state,
            server,
            server_thread: Some(server_thread),
        })
    }

    /// Address the server is listening on. Useful for tests and for
    /// printing the rendezvous URL.
    pub fn local_addr(&self) -> SocketAddr {
        self.bound
    }

    /// Single-session bearer token. Print to the operator alongside
    /// `local_addr()`; a fresh channel rotates it.
    pub fn token(&self) -> &str {
        &self.token
    }
}

impl Drop for WebApprovalChannel {
    fn drop(&mut self) {
        // tiny_http::Server::unblock breaks the recv loop so the
        // thread can exit; without it, the join would hang.
        self.server.unblock();
        if let Some(t) = self.server_thread.take() {
            let _ = t.join();
        }
    }
}

impl ApprovalChannel for WebApprovalChannel {
    fn request_approval(&mut self, req: &ApprovalRequest) -> Result<ApprovalOutcome> {
        let request_id = Uuid::now_v7().to_string();
        let deadline = Instant::now() + req.timeout;
        let mut guard = self
            .state
            .pending
            .lock()
            .map_err(|_| Error::Channel("pending mutex poisoned".to_string()))?;
        guard.insert(
            request_id.clone(),
            PendingRequest {
                request: req.clone(),
                decision: None,
            },
        );

        loop {
            if let Some(entry) = guard.get(&request_id) {
                if let Some(decision) = entry.decision.clone() {
                    guard.remove(&request_id);
                    return Ok(match decision {
                        RecordedDecision::Granted {
                            approver,
                            decided_at,
                        } => ApprovalOutcome::Granted {
                            approver_identity: approver,
                            decided_at,
                        },
                        RecordedDecision::Rejected { reason, decided_at } => {
                            ApprovalOutcome::Rejected { reason, decided_at }
                        }
                    });
                }
            }
            let now = Instant::now();
            if now >= deadline {
                guard.remove(&request_id);
                return Ok(ApprovalOutcome::TimedOut {
                    expired_at: Utc::now(),
                });
            }
            let wait_for = deadline.saturating_duration_since(now);
            let (next_guard, _) = self
                .state
                .cv
                .wait_timeout(guard, wait_for)
                .map_err(|_| Error::Channel("pending mutex poisoned".to_string()))?;
            guard = next_guard;
        }
    }
}

fn serve(server: Arc<Server>, state: Arc<State>, token: String) {
    for request in server.incoming_requests() {
        handle(&state, &token, request);
    }
}

fn handle(state: &State, token: &str, mut request: Request) {
    if !authorized(&request, token) {
        let _ = request.respond(plain(401, "missing or invalid bearer token"));
        return;
    }
    let method = request.method().clone();
    let url = request.url().to_string();
    match method {
        Method::Get if url == "/approvals" => respond_list(state, request),
        Method::Post if url.starts_with("/approvals/") => {
            let rest = &url["/approvals/".len()..];
            if let Some(id) = rest.strip_suffix("/grant") {
                resolve(state, id, true, &mut request);
                let _ = request.respond(plain(200, "ok"));
            } else if let Some(id) = rest.strip_suffix("/reject") {
                resolve(state, id, false, &mut request);
                let _ = request.respond(plain(200, "ok"));
            } else {
                let _ = request.respond(plain(404, "not found"));
            }
        }
        _ => {
            let _ = request.respond(plain(404, "not found"));
        }
    }
}

fn authorized(request: &Request, token: &str) -> bool {
    let expected = format!("Bearer {token}");
    request
        .headers()
        .iter()
        .any(|h| h.field.equiv("Authorization") && h.value.as_str() == expected)
}

fn respond_list(state: &State, request: Request) {
    let pending = match state.pending.lock() {
        Ok(p) => p,
        Err(_) => {
            let _ = request.respond(plain(500, "state poisoned"));
            return;
        }
    };
    let entries: Vec<ListEntry<'_>> = pending
        .iter()
        .filter(|(_, p)| p.decision.is_none())
        .map(|(id, p)| ListEntry {
            request_id: id,
            action_summary: &p.request.action_summary,
            resource_uri: &p.request.resource_uri,
            access_type: &p.request.access_type,
            session_id: &p.request.session_id,
            reasoning_step_id: p.request.reasoning_step_id.as_deref(),
        })
        .collect();
    let body = match serde_json::to_string(&entries) {
        Ok(b) => b,
        Err(_) => {
            let _ = request.respond(plain(500, "serialize"));
            return;
        }
    };
    let mut resp = Response::from_string(body).with_status_code(200);
    if let Some(h) = json_header() {
        resp = resp.with_header(h);
    }
    let _ = request.respond(resp);
}

fn resolve(state: &State, id: &str, grant: bool, request: &mut Request) {
    let mut body = String::new();
    let _ = request.as_reader().read_to_string(&mut body);

    let decision = if grant {
        let parsed: GrantBody = serde_json::from_str(&body).unwrap_or_default();
        RecordedDecision::Granted {
            approver: parsed.approver.unwrap_or_else(|| "web-channel".to_string()),
            decided_at: Utc::now(),
        }
    } else {
        let parsed: RejectBody = serde_json::from_str(&body).unwrap_or_default();
        RecordedDecision::Rejected {
            reason: parsed.reason.unwrap_or_else(|| "rejected".to_string()),
            decided_at: Utc::now(),
        }
    };

    let Ok(mut pending) = state.pending.lock() else {
        return;
    };
    if let Some(entry) = pending.get_mut(id) {
        entry.decision = Some(decision);
    }
    state.cv.notify_all();
}

fn plain(code: u16, msg: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(msg.to_string()).with_status_code(code)
}

fn json_header() -> Option<Header> {
    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).ok()
}

fn is_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}
