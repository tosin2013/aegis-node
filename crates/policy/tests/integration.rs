//! End-to-end tests for the policy engine.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use aegis_ledger_writer::{EntryType, LedgerWriter};
use aegis_policy::{emit_violation, NetworkProto, Policy, ViolationEvent};
use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;

/// Fixed clock used by the v0.5.0 tests that don't care about
/// time-bounded write_grants. Sessions started one minute before "now"
/// — far inside any reasonable duration window.
fn t_now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 29, 10, 1, 0).unwrap()
}
fn t_session_start() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 29, 10, 0, 0).unwrap()
}

fn read_only_research() -> Policy {
    Policy::from_yaml_file(Path::new(
        "../../schemas/manifest/v1/examples/read-only-research.manifest.yaml",
    ))
    .unwrap()
}

fn single_write_target() -> Policy {
    Policy::from_yaml_file(Path::new(
        "../../schemas/manifest/v1/examples/single-write-target.manifest.yaml",
    ))
    .unwrap()
}

#[test]
fn read_only_manifest_grants_read_under_listed_paths() {
    let p = read_only_research();
    assert!(p
        .check_filesystem_read(Path::new("/data/reports/q1.md"))
        .is_allow());
    assert!(p
        .check_filesystem_read(Path::new("/data/papers/abstract.txt"))
        .is_allow());
    // Path covered exactly equals an entry.
    assert!(p
        .check_filesystem_read(Path::new("/data/reports"))
        .is_allow());
}

#[test]
fn read_only_manifest_denies_paths_outside_grants() {
    let p = read_only_research();
    assert!(p.check_filesystem_read(Path::new("/etc/passwd")).is_deny());
    // Boundary check: /data2 must not match /data/.
    assert!(p
        .check_filesystem_read(Path::new("/data2/secret"))
        .is_deny());
}

#[test]
fn read_only_manifest_denies_all_writes() {
    let p = read_only_research();
    assert!(p
        .check_filesystem_write(Path::new("/data/reports/x"), t_now(), t_session_start())
        .is_deny());
    assert!(p
        .check_filesystem_delete(Path::new("/data/reports/x"), t_now(), t_session_start())
        .is_deny());
}

#[test]
fn read_only_manifest_denies_network() {
    let p = read_only_research();
    assert!(p
        .check_network_outbound("api.example.com", 443, NetworkProto::Https)
        .is_deny());
    assert!(p
        .check_network_inbound("0.0.0.0", 8080, NetworkProto::Tcp)
        .is_deny());
}

#[test]
fn write_grant_with_approval_returns_approval() {
    let p = single_write_target();
    // The write_grant explicitly sets approval_required: true.
    let dec = p.check_filesystem_write(
        Path::new("/data/output/daily-report.md"),
        t_now(),
        t_session_start(),
    );
    assert!(dec.is_approval(), "got {dec:?}");
}

#[test]
fn write_outside_grant_is_denied() {
    let p = single_write_target();
    assert!(p
        .check_filesystem_write(
            Path::new("/data/output/other.md"),
            t_now(),
            t_session_start()
        )
        .is_deny());
}

#[test]
fn approval_required_for_any_write_upgrades_decision() {
    // Build a tiny manifest that explicitly grants /tmp write but adds
    // any_write to approval_required_for. The grant's approval_required
    // is false; the upgrade comes from approval_required_for alone.
    let yaml = r#"schemaVersion: "1"
agent: { name: "x", version: "1.0.0" }
identity: { spiffeId: "spiffe://td/agent/x/1" }
tools:
  filesystem:
    read: ["/tmp"]
    write: ["/tmp"]
approval_required_for: ["any_write"]
"#;
    let p = Policy::from_yaml_bytes(yaml.as_bytes()).unwrap();
    let dec = p.check_filesystem_write(Path::new("/tmp/scratch"), t_now(), t_session_start());
    assert!(dec.is_approval(), "got {dec:?}");
}

#[test]
fn extends_in_phase_1a_is_unsupported() {
    let yaml = r#"schemaVersion: "1"
agent: { name: "x", version: "1.0.0" }
identity: { spiffeId: "spiffe://td/agent/x/1" }
extends: ["base.yaml"]
tools: {}
"#;
    let err = Policy::from_yaml_bytes(yaml.as_bytes()).unwrap_err();
    assert!(matches!(err, aegis_policy::Error::ExtendsUnsupported(1)));
}

#[test]
fn allowlist_matches_host_port_protocol() {
    let yaml = r#"schemaVersion: "1"
agent: { name: "x", version: "1.0.0" }
identity: { spiffeId: "spiffe://td/agent/x/1" }
tools:
  network:
    outbound:
      allowlist:
        - host: "api.example.com"
          port: 443
          protocol: "https"
"#;
    let p = Policy::from_yaml_bytes(yaml.as_bytes()).unwrap();
    assert!(p
        .check_network_outbound("api.example.com", 443, NetworkProto::Https)
        .is_allow());
    // Port mismatch.
    assert!(p
        .check_network_outbound("api.example.com", 80, NetworkProto::Https)
        .is_deny());
    // Host mismatch.
    assert!(p
        .check_network_outbound("evil.example.com", 443, NetworkProto::Https)
        .is_deny());
    // Protocol mismatch (https in manifest, plain tcp at callsite).
    assert!(p
        .check_network_outbound("api.example.com", 443, NetworkProto::Tcp)
        .is_deny());
}

#[test]
fn exec_denied_when_manifest_has_no_grants() {
    // read-only-research has no exec_grants → closed-by-default.
    let p = read_only_research();
    let dec = p.check_exec(Path::new("/usr/bin/ffmpeg"));
    assert!(dec.is_deny(), "got {dec:?}");
}

#[test]
fn exec_grant_absolute_path_matches_exact() {
    let yaml = r#"schemaVersion: "1"
agent: { name: "x", version: "1.0.0" }
identity: { spiffeId: "spiffe://td/agent/x/1" }
tools: {}
exec_grants:
  - program: "/usr/bin/git"
"#;
    let p = Policy::from_yaml_bytes(yaml.as_bytes()).unwrap();
    assert!(p.check_exec(Path::new("/usr/bin/git")).is_allow());
    assert!(p.check_exec(Path::new("/usr/local/bin/git")).is_deny());
}

#[test]
fn exec_grant_basename_matches_anywhere() {
    let yaml = r#"schemaVersion: "1"
agent: { name: "x", version: "1.0.0" }
identity: { spiffeId: "spiffe://td/agent/x/1" }
tools: {}
exec_grants:
  - program: "ffmpeg"
"#;
    let p = Policy::from_yaml_bytes(yaml.as_bytes()).unwrap();
    assert!(p.check_exec(Path::new("/usr/bin/ffmpeg")).is_allow());
    assert!(p.check_exec(Path::new("/snap/bin/ffmpeg")).is_allow());
    assert!(p.check_exec(Path::new("/usr/bin/curl")).is_deny());
}

#[test]
fn exec_any_exec_upgrades_match_to_approval() {
    let yaml = r#"schemaVersion: "1"
agent: { name: "x", version: "1.0.0" }
identity: { spiffeId: "spiffe://td/agent/x/1" }
tools: {}
exec_grants:
  - program: "/usr/bin/git"
approval_required_for: ["any_exec"]
"#;
    let p = Policy::from_yaml_bytes(yaml.as_bytes()).unwrap();
    assert!(p.check_exec(Path::new("/usr/bin/git")).is_approval());
    // any_exec must NOT promote a deny into approval.
    assert!(p.check_exec(Path::new("/usr/bin/curl")).is_deny());
}

#[test]
fn emit_violation_appends_violation_entry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("violations.jsonl");
    let mut writer = LedgerWriter::create(&path, "session-policy".to_string()).unwrap();

    let ts = Utc.with_ymd_and_hms(2026, 4, 28, 12, 0, 0).unwrap();
    let event = ViolationEvent::for_network(
        "evil.example.com",
        443,
        NetworkProto::Https,
        "host not in allowlist",
        ts,
    );
    let rec = emit_violation(&mut writer, [0xCCu8; 32], event).unwrap();
    assert_eq!(rec.sequence_number, 0);
    writer.close().unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let v: Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    assert_eq!(v["entryType"], "violation");
    assert_eq!(v["violationReason"], "host not in allowlist");
    assert_eq!(v["resourceUri"], "https://evil.example.com:443");
    assert_eq!(v["accessType"], "network_outbound");

    let _ = EntryType::Violation; // sanity
}

/// Per ADR-018 / issue #46. Loads the agent-with-mcp example (research
/// agent + Anthropic filesystem MCP server, read-only subset) and
/// asserts the engine agrees with the manifest's intent.
#[test]
fn agent_with_mcp_example_enforces_read_only_subset() {
    let p = Policy::from_yaml_file(Path::new(
        "../../schemas/manifest/v1/examples/agent-with-mcp.manifest.yaml",
    ))
    .unwrap();

    // Listed tool: read_text_file => allow.
    assert!(p.check_mcp_tool("filesystem", "read_text_file").is_allow());

    // Same server, but write_file is deliberately omitted from
    // allowed_tools — the example is read-only.
    assert!(p.check_mcp_tool("filesystem", "write_file").is_deny());

    // Server not listed => deny regardless of tool name.
    assert!(p.check_mcp_tool("evil-server", "read_text_file").is_deny());
}

/// Per ADR-018 / issue #43. Parses a valid `tools.mcp[]` example.
#[test]
fn mcp_server_grant_parses() {
    let yaml = br#"
schemaVersion: "1"
agent: { name: "x", version: "1.0.0" }
identity: { spiffeId: "spiffe://td/agent/x/1" }
tools:
  mcp:
    - server_name: "fs-helper"
      server_uri: "stdio:/usr/local/bin/mcp-fs"
      allowed_tools: ["read_file", "list_dir"]
    - server_name: "web-search"
      server_uri: "https://mcp.example.com:8443"
      allowed_tools: []
"#;
    let p = Policy::from_yaml_bytes(yaml).unwrap();
    let mcp = &p.manifest().tools.mcp;
    assert_eq!(mcp.len(), 2);
    assert_eq!(mcp[0].server_name, "fs-helper");
    assert_eq!(mcp[0].server_uri, "stdio:/usr/local/bin/mcp-fs");
    // Both shorthand entries deserialize as AllowedTool::Name; the
    // accessor lets call sites stay name-only when they don't care
    // about pre_validate clauses (which are absent here).
    let names: Vec<&str> = mcp[0].allowed_tools.iter().map(|t| t.name()).collect();
    assert_eq!(names, vec!["read_file", "list_dir"]);
    for entry in &mcp[0].allowed_tools {
        assert!(entry.pre_validate().is_empty(), "shorthand should have no clauses");
    }
    assert_eq!(mcp[1].server_name, "web-search");
    assert!(mcp[1].allowed_tools.is_empty());
}

/// Per ADR-024-A. The object form of an `allowed_tools` entry
/// declares per-tool pre-validation clauses. Both shapes can coexist
/// in the same array (one entry as a bare name, another as an
/// object) — the union deserializes via `#[serde(untagged)]`.
#[test]
fn mcp_allowed_tools_accepts_pre_validate_object_form() {
    use aegis_policy::manifest::{AllowedTool, PreValidateKind};

    let yaml = br#"
schemaVersion: "1"
agent: { name: "x", version: "1.0.0" }
identity: { spiffeId: "spiffe://td/agent/x/1" }
tools:
  mcp:
    - server_name: "fs"
      server_uri: "stdio:/usr/local/bin/mcp-fs"
      allowed_tools:
        - "list_directory"
        - name: "read_text_file"
          pre_validate:
            - kind: filesystem_read
              arg: path
        - name: "read_multiple_files"
          pre_validate:
            - kind: filesystem_read
              arg_array: paths
        - name: "write_file"
          pre_validate:
            - kind: filesystem_write
              arg: path
"#;
    let p = Policy::from_yaml_bytes(yaml).unwrap();
    let entries = &p.manifest().tools.mcp[0].allowed_tools;
    assert_eq!(entries.len(), 4);

    // Mixed shapes coexist in the array.
    match &entries[0] {
        AllowedTool::Name(n) => assert_eq!(n, "list_directory"),
        other => panic!("expected shorthand, got {other:?}"),
    }
    match &entries[1] {
        AllowedTool::WithPreValidate { name, pre_validate } => {
            assert_eq!(name, "read_text_file");
            assert_eq!(pre_validate.len(), 1);
            assert_eq!(pre_validate[0].kind, PreValidateKind::FilesystemRead);
            assert_eq!(pre_validate[0].arg.as_deref(), Some("path"));
            assert_eq!(pre_validate[0].arg_array, None);
        }
        other => panic!("expected object form, got {other:?}"),
    }

    // arg_array variant.
    match &entries[2] {
        AllowedTool::WithPreValidate { name, pre_validate } => {
            assert_eq!(name, "read_multiple_files");
            assert_eq!(pre_validate[0].kind, PreValidateKind::FilesystemRead);
            assert_eq!(pre_validate[0].arg, None);
            assert_eq!(pre_validate[0].arg_array.as_deref(), Some("paths"));
        }
        other => panic!("expected object form, got {other:?}"),
    }

    // The MCP allowlist check still uses the name — pre_validate
    // clauses don't affect the allow/deny decision (per the
    // ADR-024-B mediator pass which runs separately).
    assert!(p.check_mcp_tool("fs", "read_text_file").is_allow());
    assert!(p.check_mcp_tool("fs", "list_directory").is_allow());
    assert!(p.check_mcp_tool("fs", "delete_file").is_deny());
}

/// Per ADR-018 / issue #43. Malformed entry (missing `server_uri`) is rejected.
#[test]
fn mcp_server_grant_rejects_missing_uri() {
    let yaml = br#"
schemaVersion: "1"
agent: { name: "x", version: "1.0.0" }
identity: { spiffeId: "spiffe://td/agent/x/1" }
tools:
  mcp:
    - server_name: "fs-helper"
      allowed_tools: ["read_file"]
"#;
    let err = Policy::from_yaml_bytes(yaml).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("server_uri") || msg.contains("missing"),
        "error should mention the missing field: {msg}",
    );
}
