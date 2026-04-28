//! Aegis-Node network gate.
//!
//! Runtime-level network-deny enforcement per ADR-008 (F6) and ADR-004 (F2).
//! This crate provides thin wrappers around `std::net` that consult a
//! [`aegis_policy::Policy`] before allowing a connect/bind. Code that uses
//! [`AegisTcpStream::connect`] (and future [`AegisTcpListener`], etc.) gets
//! deny-by-default network behavior; code that calls `std::net` directly is
//! out of scope — sandbox layers (issue #7's filesystem follow-up + future
//! seccomp hardening) are what stop bypass attempts.
//!
//! Phase 1a wraps only outbound TCP connect. Inbound listeners, UDP, and
//! exec gating land with the rest of the F2 enforcer.

use std::net::{TcpStream, ToSocketAddrs};

use aegis_policy::{Decision, NetworkProto, Policy};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("network policy denied: {reason}")]
    Denied { reason: String },

    #[error("network policy requires approval: {reason}")]
    RequireApproval { reason: String },
}

pub type Result<T> = std::result::Result<T, Error>;

/// Outbound TCP gate. Constructed with a borrowed [`Policy`]; the policy
/// stays alive for as long as the gate handle does.
pub struct AegisTcpStream;

impl AegisTcpStream {
    /// Policy-checked drop-in replacement for `TcpStream::connect`.
    /// `proto` lets the caller declare semantic intent (HTTPS vs plain
    /// TCP) so the manifest's allowlist can match by protocol.
    pub fn connect(
        policy: &Policy,
        host: &str,
        port: u16,
        proto: NetworkProto,
    ) -> Result<TcpStream> {
        match policy.check_network_outbound(host, port, proto) {
            Decision::Allow => {}
            Decision::Deny { reason } => return Err(Error::Denied { reason }),
            Decision::RequireApproval { reason } => {
                return Err(Error::RequireApproval { reason })
            }
        }

        let addr = (host, port);
        let stream = addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("could not resolve {host}:{port}"),
                )
            })
            .and_then(TcpStream::connect)?;
        Ok(stream)
    }
}
