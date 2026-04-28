//! End-to-end tests for the AegisTcpStream wrapper.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use aegis_network_gate::{AegisTcpStream, Error};
use aegis_policy::{NetworkProto, Policy};

fn policy_with_outbound(yaml_outbound: &str) -> Policy {
    let yaml = format!(
        r#"schemaVersion: "1"
agent: {{ name: "x", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://td/agent/x/1" }}
tools:
  network:
    outbound: {yaml_outbound}
"#
    );
    Policy::from_yaml_bytes(yaml.as_bytes()).unwrap()
}

#[test]
fn deny_mode_blocks_connect() {
    let p = policy_with_outbound("deny");
    let err = AegisTcpStream::connect(&p, "127.0.0.1", 1, NetworkProto::Tcp).unwrap_err();
    assert!(matches!(err, Error::Denied { .. }));
}

#[test]
fn missing_network_section_blocks_connect() {
    let yaml = r#"schemaVersion: "1"
agent: { name: "x", version: "1.0.0" }
identity: { spiffeId: "spiffe://td/agent/x/1" }
tools: {}
"#;
    let p = Policy::from_yaml_bytes(yaml.as_bytes()).unwrap();
    let err = AegisTcpStream::connect(&p, "127.0.0.1", 1, NetworkProto::Tcp).unwrap_err();
    assert!(matches!(err, Error::Denied { .. }));
}

#[test]
fn allowlist_match_permits_connect() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut buf = [0u8; 4];
        let _ = s.read(&mut buf);
        s.write_all(b"ok\n").unwrap();
    });

    let outbound = format!(
        r#"
      allowlist:
        - host: "127.0.0.1"
          port: {port}
          protocol: "tcp""#
    );
    let p = policy_with_outbound(&outbound);
    let mut stream = AegisTcpStream::connect(&p, "127.0.0.1", port, NetworkProto::Tcp).unwrap();
    stream.write_all(b"ping").unwrap();
    let mut resp = [0u8; 3];
    stream.read_exact(&mut resp).unwrap();
    assert_eq!(&resp, b"ok\n");

    server.join().unwrap();
}

#[test]
fn allowlist_miss_blocks_connect() {
    let outbound = r#"
      allowlist:
        - host: "127.0.0.1"
          port: 65000
          protocol: "tcp""#;
    let p = policy_with_outbound(outbound);
    // Different port than the allowlist entry.
    let err = AegisTcpStream::connect(&p, "127.0.0.1", 1, NetworkProto::Tcp).unwrap_err();
    assert!(matches!(err, Error::Denied { .. }));
}
