//! Cross-language conformance harness (issue #16) — Rust side.
//!
//! Loads tests/conformance/cases.json and asserts the Rust enforcer
//! agrees with every expected decision. The Go side runs the same
//! fixture in `pkg/manifest`. If either disagrees, that side's CI
//! fails — and since both pass against the same fixture, transitively
//! both engines agree.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use aegis_policy::{Decision, NetworkProto, Policy};
use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;

/// Default clock for cases that don't pin one. Sessions started one
/// minute before "now" — far inside any reasonable duration window so
/// pre-#38 fixtures keep behaving as if grants were unbounded.
fn default_now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 29, 10, 1, 0).unwrap()
}
fn default_session_start() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 29, 10, 0, 0).unwrap()
}

const CASES_PATH: &str = "../../tests/conformance/cases.json";

#[derive(Debug, Deserialize)]
struct ConformanceFile {
    version: String,
    cases: Vec<ConformanceCase>,
}

#[derive(Debug, Deserialize)]
struct ConformanceCase {
    name: String,
    manifest: String,
    query: Query,
    expected: ExpectedKind,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Query {
    FilesystemRead {
        resource_uri: String,
    },
    FilesystemWrite {
        resource_uri: String,
        #[serde(default)]
        now: Option<DateTime<Utc>>,
        #[serde(default)]
        session_start: Option<DateTime<Utc>>,
    },
    FilesystemDelete {
        resource_uri: String,
        #[serde(default)]
        now: Option<DateTime<Utc>>,
        #[serde(default)]
        session_start: Option<DateTime<Utc>>,
    },
    NetworkOutbound {
        host: String,
        port: u16,
        protocol: ProtocolStr,
    },
    NetworkInbound {
        host: String,
        port: u16,
        protocol: ProtocolStr,
    },
    Exec {
        #[serde(default)]
        resource_uri: String,
    },
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum ProtocolStr {
    Http,
    Https,
    Tcp,
    Udp,
}

impl From<ProtocolStr> for NetworkProto {
    fn from(p: ProtocolStr) -> Self {
        match p {
            ProtocolStr::Http => NetworkProto::Http,
            ProtocolStr::Https => NetworkProto::Https,
            ProtocolStr::Tcp => NetworkProto::Tcp,
            ProtocolStr::Udp => NetworkProto::Udp,
        }
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ExpectedKind {
    Allow,
    Deny,
    RequireApproval,
}

fn decision_kind(d: &Decision) -> ExpectedKind {
    match d {
        Decision::Allow => ExpectedKind::Allow,
        Decision::Deny { .. } => ExpectedKind::Deny,
        Decision::RequireApproval { .. } => ExpectedKind::RequireApproval,
    }
}

#[test]
fn conformance_rust_side() {
    let cases_path = Path::new(CASES_PATH);
    let cases_dir = cases_path.parent().unwrap();
    let raw = std::fs::read_to_string(cases_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", cases_path.display()));
    let file: ConformanceFile = serde_json::from_str(&raw).expect("parse cases.json");
    assert_eq!(file.version, "1", "unexpected fixture version");
    assert!(!file.cases.is_empty(), "no cases in fixture");

    let mut failed: Vec<String> = Vec::new();
    for case in &file.cases {
        let manifest_path: PathBuf = cases_dir.join(&case.manifest);
        let policy = Policy::from_yaml_file(&manifest_path)
            .unwrap_or_else(|e| panic!("load {}: {e}", manifest_path.display()));

        let decision = match &case.query {
            Query::FilesystemRead { resource_uri } => {
                policy.check_filesystem_read(Path::new(resource_uri))
            }
            Query::FilesystemWrite {
                resource_uri,
                now,
                session_start,
            } => policy.check_filesystem_write(
                Path::new(resource_uri),
                now.unwrap_or_else(default_now),
                session_start.unwrap_or_else(default_session_start),
            ),
            Query::FilesystemDelete {
                resource_uri,
                now,
                session_start,
            } => policy.check_filesystem_delete(
                Path::new(resource_uri),
                now.unwrap_or_else(default_now),
                session_start.unwrap_or_else(default_session_start),
            ),
            Query::NetworkOutbound {
                host,
                port,
                protocol,
            } => policy.check_network_outbound(host, *port, (*protocol).into()),
            Query::NetworkInbound {
                host,
                port,
                protocol,
            } => policy.check_network_inbound(host, *port, (*protocol).into()),
            Query::Exec { resource_uri } => policy.check_exec(Path::new(resource_uri)),
        };

        let actual = decision_kind(&decision);
        if actual != case.expected {
            failed.push(format!(
                "case {} drift: want {:?} got {:?} (reason: {:?})",
                case.name, case.expected, actual, decision
            ));
        }
    }

    assert!(
        failed.is_empty(),
        "{} conformance case(s) failed:\n  {}",
        failed.len(),
        failed.join("\n  ")
    );
}
