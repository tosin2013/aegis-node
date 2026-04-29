//! End-to-end tests for the mTLS signed-API approval channel
//! (issue #36, F3).
//!
//! Each test issues server + client SVIDs from a fresh `LocalCa`, spins
//! up an `MtlsApprovalChannel`, and connects a tokio-rustls client to
//! either grant or reject — or simply disconnects to exercise the
//! timeout / unauthorized paths.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use aegis_approval_gate::{
    ApprovalChannel, ApprovalOutcome, ApprovalRequest, MtlsApprovalChannel,
};
use aegis_identity::{Digest, DigestTriple, LocalCa};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use rustls_pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

const TRUST_DOMAIN: &str = "mtls-approval.local";

fn ephemeral_digests() -> DigestTriple {
    DigestTriple {
        model: Digest([1u8; 32]),
        manifest: Digest([2u8; 32]),
        config: Digest([3u8; 32]),
    }
}

fn fresh_request(timeout_ms: u64) -> ApprovalRequest {
    ApprovalRequest {
        action_summary: "delete /etc/secrets".to_string(),
        resource_uri: "file:///etc/secrets".to_string(),
        access_type: "delete".to_string(),
        session_id: "session-mtls-test".to_string(),
        reasoning_step_id: Some("rstep-007".to_string()),
        timeout: Duration::from_millis(timeout_ms),
    }
}

struct ApprovalParty {
    cert_pem: String,
    key_pem: String,
    spiffe_uri: String,
}

fn issue(ca: &LocalCa, workload: &str, instance: &str) -> ApprovalParty {
    let svid = ca.issue_svid(workload, instance, ephemeral_digests()).unwrap();
    let spiffe_uri = svid.spiffe_id.uri();
    ApprovalParty {
        cert_pem: svid.cert_pem,
        key_pem: svid.key_pem,
        spiffe_uri,
    }
}

fn parse_certs(pem: &str) -> Vec<CertificateDer<'static>> {
    let mut reader = std::io::BufReader::new(pem.as_bytes());
    rustls_pemfile::certs(&mut reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap()
}

fn parse_private_key(pem: &str) -> PrivateKeyDer<'static> {
    let mut reader = std::io::BufReader::new(pem.as_bytes());
    rustls_pemfile::private_key(&mut reader).unwrap().unwrap()
}

/// Test-only verifier: trusts any server cert. The channel under test
/// validates the *client* cert's SPIFFE ID; we don't separately verify
/// the server identity here. Real callers wire up a SPIFFE-aware
/// server-cert verifier instead.
#[derive(Debug)]
struct AcceptAnyServerCert;

impl ServerCertVerifier for AcceptAnyServerCert {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
        ]
    }
}

fn build_client_config(
    _ca_root_pem: &str,
    client_cert_pem: &str,
    client_key_pem: &str,
) -> ClientConfig {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyServerCert))
        .with_client_auth_cert(parse_certs(client_cert_pem), parse_private_key(client_key_pem))
        .unwrap()
}

async fn drive_decision(
    addr: SocketAddr,
    config: ClientConfig,
    decision_line: &str,
) -> std::io::Result<String> {
    let stream = TcpStream::connect(addr).await?;
    let connector = TlsConnector::from(Arc::new(config));
    let server_name = ServerName::IpAddress(addr.ip().into());
    let tls = connector.connect(server_name, stream).await?;
    let mut reader = BufReader::new(tls);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).await?;
    let mut tls = reader.into_inner();
    tls.write_all(decision_line.as_bytes()).await?;
    tls.write_all(b"\n").await?;
    tls.shutdown().await.ok();
    Ok(request_line)
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .unwrap()
}

#[test]
fn granted_decision_returns_approver_spiffe_id() {
    let ca_dir = tempfile::tempdir().unwrap();
    let ca = LocalCa::init(ca_dir.path(), TRUST_DOMAIN).unwrap();
    let server = issue(&ca, "research", "inst-1");
    let approver = issue(&ca, "approver-bot", "ops-1");

    let mut channel = MtlsApprovalChannel::new(
        "127.0.0.1:0",
        &server.cert_pem,
        &server.key_pem,
        &ca.root_cert_pem(),
        vec![approver.spiffe_uri.clone()],
    )
    .unwrap();
    let addr = channel.local_addr();

    let client_cfg = build_client_config(&ca.root_cert_pem(), &approver.cert_pem, &approver.key_pem);
    let driver = std::thread::spawn(move || {
        rt().block_on(drive_decision(
            addr,
            client_cfg,
            r#"{"decision":"granted"}"#,
        ))
    });

    let outcome = channel.request_approval(&fresh_request(5_000)).unwrap();
    let request_line = driver.join().unwrap().unwrap();
    assert!(request_line.contains("session-mtls-test"));
    match outcome {
        ApprovalOutcome::Granted {
            approver_identity, ..
        } => {
            assert_eq!(approver_identity, approver.spiffe_uri);
        }
        other => panic!("expected Granted, got {other:?}"),
    }
}

#[test]
fn rejected_decision_carries_reason() {
    let ca_dir = tempfile::tempdir().unwrap();
    let ca = LocalCa::init(ca_dir.path(), TRUST_DOMAIN).unwrap();
    let server = issue(&ca, "research", "inst-1");
    let approver = issue(&ca, "approver-bot", "ops-1");

    let mut channel = MtlsApprovalChannel::new(
        "127.0.0.1:0",
        &server.cert_pem,
        &server.key_pem,
        &ca.root_cert_pem(),
        vec![approver.spiffe_uri],
    )
    .unwrap();
    let addr = channel.local_addr();

    let client_cfg = build_client_config(&ca.root_cert_pem(), &approver.cert_pem, &approver.key_pem);
    let driver = std::thread::spawn(move || {
        rt().block_on(drive_decision(
            addr,
            client_cfg,
            r#"{"decision":"rejected","reason":"out of policy"}"#,
        ))
    });

    let outcome = channel.request_approval(&fresh_request(5_000)).unwrap();
    let _ = driver.join().unwrap();
    match outcome {
        ApprovalOutcome::Rejected { reason, .. } => assert_eq!(reason, "out of policy"),
        other => panic!("expected Rejected, got {other:?}"),
    }
}

#[test]
fn unauthorized_spiffe_id_is_ignored_until_timeout() {
    let ca_dir = tempfile::tempdir().unwrap();
    let ca = LocalCa::init(ca_dir.path(), TRUST_DOMAIN).unwrap();
    let server = issue(&ca, "research", "inst-1");
    let allowed_approver = issue(&ca, "approver-bot", "ops-1");
    let intruder = issue(&ca, "rogue-bot", "elsewhere-1");

    // Allowlist contains only the legitimate approver.
    let mut channel = MtlsApprovalChannel::new(
        "127.0.0.1:0",
        &server.cert_pem,
        &server.key_pem,
        &ca.root_cert_pem(),
        vec![allowed_approver.spiffe_uri],
    )
    .unwrap();
    let addr = channel.local_addr();

    let intruder_cfg = build_client_config(&ca.root_cert_pem(), &intruder.cert_pem, &intruder.key_pem);
    let driver = std::thread::spawn(move || {
        // The intruder's mTLS handshake succeeds (cert chains to the CA)
        // but the server should close before sending the request body.
        let _ = rt().block_on(drive_decision(
            addr,
            intruder_cfg,
            r#"{"decision":"granted"}"#,
        ));
    });

    let started = std::time::Instant::now();
    let outcome = channel.request_approval(&fresh_request(800)).unwrap();
    let elapsed = started.elapsed();
    let _ = driver.join();
    assert!(
        matches!(outcome, ApprovalOutcome::TimedOut { .. }),
        "non-allowlisted SPIFFE ID must not decide; got {outcome:?}"
    );
    assert!(
        elapsed >= Duration::from_millis(700),
        "should hold open until the timeout: {elapsed:?}"
    );
}

#[test]
fn timeout_when_no_client_connects() {
    let ca_dir = tempfile::tempdir().unwrap();
    let ca = LocalCa::init(ca_dir.path(), TRUST_DOMAIN).unwrap();
    let server = issue(&ca, "research", "inst-1");
    let approver = issue(&ca, "approver-bot", "ops-1");

    let mut channel = MtlsApprovalChannel::new(
        "127.0.0.1:0",
        &server.cert_pem,
        &server.key_pem,
        &ca.root_cert_pem(),
        vec![approver.spiffe_uri],
    )
    .unwrap();

    let started = std::time::Instant::now();
    let outcome = channel.request_approval(&fresh_request(300)).unwrap();
    let elapsed = started.elapsed();
    assert!(matches!(outcome, ApprovalOutcome::TimedOut { .. }));
    assert!(elapsed >= Duration::from_millis(250));
}
