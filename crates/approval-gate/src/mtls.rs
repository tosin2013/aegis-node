//! mTLS signed-API approval channel (F3, issue #36).
//!
//! A long-running TLS server that:
//!
//! 1. requires every client to present an X.509 certificate chained to
//!    a configured CA root,
//! 2. extracts the client's SPIFFE ID from the leaf cert's URI SAN, and
//! 3. accepts approval decisions only from clients whose SPIFFE ID is
//!    listed in `manifest.approval_authorities`.
//!
//! Wire protocol (one connection per approval round):
//!
//! ```text
//! server -> client: {"action_summary": "...", "resource_uri": "...",
//!                    "access_type": "...", "session_id": "...",
//!                    "reasoning_step_id": "..."}\n
//! client -> server: {"decision": "granted"|"rejected",
//!                    "reason": "..."}\n
//! ```
//!
//! Authorization failures (cert not chained, SPIFFE ID not in allowlist)
//! close the connection silently and the server keeps waiting for the
//! next attempt. The `request_approval` call returns `TimedOut` if no
//! authorized decision arrives before `req.timeout`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_rustls::TlsAcceptor;

use crate::{ApprovalChannel, ApprovalOutcome, ApprovalRequest, Error, Result};

/// One mTLS-channel construction. Owns a bound listener so the operating
/// system port is allocated once; subsequent `request_approval` calls
/// re-use the same socket.
pub struct MtlsApprovalChannel {
    listener: tokio::net::TcpListener,
    acceptor: TlsAcceptor,
    allowlist: Vec<String>,
    runtime: tokio::runtime::Runtime,
    bound: SocketAddr,
}

impl std::fmt::Debug for MtlsApprovalChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MtlsApprovalChannel")
            .field("bound", &self.bound)
            .field("allowlist_len", &self.allowlist.len())
            .finish_non_exhaustive()
    }
}

impl MtlsApprovalChannel {
    /// Build a channel bound to `bind_addr` (use `127.0.0.1:0` to let
    /// the kernel pick a port). `server_cert_pem`/`server_key_pem` is
    /// the server's own SVID; `ca_root_pem` is the CA used to verify
    /// presented client certs; `allowlist` is the set of approver
    /// SPIFFE URIs allowed to grant/reject.
    pub fn new(
        bind_addr: &str,
        server_cert_pem: &str,
        server_key_pem: &str,
        ca_root_pem: &str,
        allowlist: Vec<String>,
    ) -> Result<Self> {
        let server_certs = parse_certs(server_cert_pem)?;
        let server_key = parse_private_key(server_key_pem)?;
        let ca_certs = parse_certs(ca_root_pem)?;

        let mut roots = RootCertStore::empty();
        for c in &ca_certs {
            roots
                .add(c.clone())
                .map_err(|e| Error::Channel(format!("add CA cert: {e}")))?;
        }
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier =
            WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider.clone())
                .build()
                .map_err(|e| Error::Channel(format!("client verifier: {e}")))?;

        let cfg = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| Error::Channel(format!("server config protocol: {e}")))?
            .with_client_cert_verifier(verifier)
            .with_single_cert(server_certs, server_key)
            .map_err(|e| Error::Channel(format!("server config: {e}")))?;

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .map_err(Error::Io)?;

        let (listener, bound) = runtime.block_on(async {
            let l = TcpListener::bind(bind_addr).await.map_err(Error::Io)?;
            let a = l.local_addr().map_err(Error::Io)?;
            Ok::<_, Error>((l, a))
        })?;

        Ok(Self {
            listener,
            acceptor: TlsAcceptor::from(Arc::new(cfg)),
            allowlist,
            runtime,
            bound,
        })
    }

    /// The actual port the channel listens on. Useful for tests and for
    /// surfacing the rendezvous URL to the operator.
    pub fn local_addr(&self) -> SocketAddr {
        self.bound
    }
}

/// JSON payload from a client with its decision.
#[derive(Debug, Deserialize)]
struct ClientDecision {
    decision: String,
    #[serde(default)]
    reason: Option<String>,
}

impl ApprovalChannel for MtlsApprovalChannel {
    fn request_approval(&mut self, req: &ApprovalRequest) -> Result<ApprovalOutcome> {
        let listener = &self.listener;
        let acceptor = &self.acceptor;
        let allowlist = &self.allowlist;
        let req_payload = serde_json::to_string(req)?;

        self.runtime.block_on(async move {
            let deadline = req.timeout;
            match timeout(
                deadline,
                accept_loop(listener, acceptor, allowlist, &req_payload),
            )
            .await
            {
                Ok(result) => result,
                Err(_) => Ok(ApprovalOutcome::TimedOut {
                    expired_at: Utc::now(),
                }),
            }
        })
    }
}

async fn accept_loop(
    listener: &TcpListener,
    acceptor: &TlsAcceptor,
    allowlist: &[String],
    req_payload: &str,
) -> Result<ApprovalOutcome> {
    loop {
        let (stream, _peer) = match listener.accept().await {
            Ok(p) => p,
            Err(e) => return Err(Error::Io(e)),
        };
        let acceptor = acceptor.clone();
        let mut tls = match acceptor.accept(stream).await {
            Ok(t) => t,
            Err(_) => continue, // bad handshake — ignore, wait for next
        };

        let approver = match approver_spiffe_id(&tls) {
            Some(s) => s,
            None => {
                let _ = tls.shutdown().await;
                continue;
            }
        };
        if !allowlist.iter().any(|allowed| allowed == &approver) {
            let _ = tls.shutdown().await;
            continue;
        }

        if tls.write_all(req_payload.as_bytes()).await.is_err()
            || tls.write_all(b"\n").await.is_err()
        {
            continue;
        }
        let mut reader = BufReader::new(tls);
        let mut line = String::new();
        if reader.read_line(&mut line).await.is_err() {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let decision: ClientDecision = match serde_json::from_str(trimmed) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let now = Utc::now();
        return match decision.decision.as_str() {
            "granted" => Ok(ApprovalOutcome::Granted {
                approver_identity: approver,
                decided_at: now,
            }),
            "rejected" => Ok(ApprovalOutcome::Rejected {
                reason: decision.reason.unwrap_or_else(|| "rejected".to_string()),
                decided_at: now,
            }),
            other => Err(Error::Malformed(format!(
                "decision must be \"granted\" or \"rejected\", got {other:?}"
            ))),
        };
    }
}

fn approver_spiffe_id<S>(tls: &tokio_rustls::server::TlsStream<S>) -> Option<String> {
    let (_, conn) = tls.get_ref();
    let certs = conn.peer_certificates()?;
    let leaf = certs.first()?;
    extract_spiffe_uri(leaf.as_ref())
}

fn extract_spiffe_uri(der: &[u8]) -> Option<String> {
    use x509_parser::extensions::{GeneralName, ParsedExtension};
    use x509_parser::parse_x509_certificate;

    let (_, cert) = parse_x509_certificate(der).ok()?;
    for ext in cert.extensions() {
        if let ParsedExtension::SubjectAlternativeName(san) = ext.parsed_extension() {
            for name in &san.general_names {
                if let GeneralName::URI(uri) = name {
                    if uri.starts_with("spiffe://") {
                        return Some((*uri).to_string());
                    }
                }
            }
        }
    }
    None
}

fn parse_certs(pem: &str) -> Result<Vec<CertificateDer<'static>>> {
    let mut reader = std::io::BufReader::new(pem.as_bytes());
    rustls_pemfile::certs(&mut reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Channel(format!("parse certs: {e}")))
}

fn parse_private_key(pem: &str) -> Result<PrivateKeyDer<'static>> {
    let mut reader = std::io::BufReader::new(pem.as_bytes());
    rustls_pemfile::private_key(&mut reader)
        .map_err(|e| Error::Channel(format!("parse private key: {e}")))?
        .ok_or_else(|| Error::Channel("no private key in PEM".to_string()))
}

/// Default per-call deadline for the mTLS channel. Mirrors
/// `crate::DEFAULT_TIMEOUT` so the public surface stays consistent.
pub const DEFAULT_MTLS_TIMEOUT: Duration = Duration::from_secs(60);
