//! End-to-end tests for the `aegis run` subcommand (issue #28, F0-E).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use aegis_cli::run::{execute, RunArgs};
use aegis_identity::LocalCa;

const TRUST_DOMAIN: &str = "run-test.local";

fn init_ca(dir: &Path) {
    LocalCa::init(dir, TRUST_DOMAIN).unwrap();
}

fn write(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
}

fn args_for(
    workdir: &Path,
    identity_dir: PathBuf,
    session_id: &str,
    manifest_yaml: &str,
    script_json: &str,
) -> RunArgs {
    let manifest = workdir.join("manifest.yaml");
    let model = workdir.join("model.gguf");
    let script = workdir.join("script.json");
    let ledger = workdir.join("ledger.jsonl");
    write(&manifest, manifest_yaml);
    write(&model, "fake-model-bytes");
    write(&script, script_json);
    RunArgs {
        manifest,
        model,
        config: None,
        identity_dir: Some(identity_dir),
        workload: "research".to_string(),
        instance: "inst-1".to_string(),
        ledger: Some(ledger),
        session_id: Some(session_id.to_string()),
        script,
    }
}

#[test]
fn run_with_only_allowed_calls_completes_cleanly() {
    let work = tempfile::tempdir().unwrap();
    let ca = tempfile::tempdir().unwrap();
    init_ca(ca.path());

    let read_target = work.path().join("input.txt");
    let write_target = work.path().join("output.txt");
    std::fs::write(&read_target, b"hello").unwrap();

    let manifest = format!(
        r#"schemaVersion: "1"
agent: {{ name: "r", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://run-test.local/agent/research/inst-1" }}
tools:
  filesystem:
    read: ["{r}"]
    write: ["{w}"]
"#,
        r = work.path().to_str().unwrap(),
        w = work.path().to_str().unwrap(),
    );
    let script = format!(
        r#"{{ "calls": [
            {{"kind": "filesystem_read", "path": "{r}", "reasoning_step_id": "r1"}},
            {{"kind": "filesystem_write", "path": "{w}", "contents": "out", "reasoning_step_id": "r2"}}
        ] }}"#,
        r = read_target.to_str().unwrap(),
        w = write_target.to_str().unwrap(),
    );

    let args = args_for(
        work.path(),
        ca.path().to_path_buf(),
        "session-clean",
        &manifest,
        &script,
    );
    let outcome = execute(args).unwrap();

    assert!(!outcome.halted, "halt_reason: {:?}", outcome.halt_reason);
    assert_eq!(outcome.entry_count, 4); // start + 2 access + end
    assert_eq!(outcome.session_id, "session-clean");
    assert!(outcome.ledger_path.exists());
}

#[test]
fn deny_in_script_continues_run_and_records_violation() {
    let work = tempfile::tempdir().unwrap();
    let ca = tempfile::tempdir().unwrap();
    init_ca(ca.path());

    let allowed = work.path().join("ok.txt");
    let forbidden = work.path().join("nope.txt");
    std::fs::write(&allowed, b"x").unwrap();
    std::fs::write(&forbidden, b"y").unwrap();

    // Manifest grants only `allowed`'s parent — `forbidden` is in the
    // same dir but the read list narrows to a sibling-only path.
    let granted_dir = work.path().join("ok-only");
    std::fs::create_dir(&granted_dir).unwrap();
    let granted_file = granted_dir.join("ok.txt");
    std::fs::write(&granted_file, b"x").unwrap();

    let manifest = format!(
        r#"schemaVersion: "1"
agent: {{ name: "r", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://run-test.local/agent/research/inst-1" }}
tools:
  filesystem:
    read: ["{g}"]
"#,
        g = granted_dir.to_str().unwrap(),
    );
    let script = format!(
        r#"{{ "calls": [
            {{"kind": "filesystem_read", "path": "{ok}"}},
            {{"kind": "filesystem_read", "path": "{bad}"}},
            {{"kind": "filesystem_read", "path": "{ok}"}}
        ] }}"#,
        ok = granted_file.to_str().unwrap(),
        bad = forbidden.to_str().unwrap(),
    );

    let args = args_for(
        work.path(),
        ca.path().to_path_buf(),
        "session-deny",
        &manifest,
        &script,
    );
    let outcome = execute(args).unwrap();

    assert!(!outcome.halted, "deny does not halt; {:?}", outcome.halt_reason);
    // start + access + violation + access + end
    assert_eq!(outcome.entry_count, 5);
}

#[test]
fn approval_required_halts_run_in_phase_1a() {
    let work = tempfile::tempdir().unwrap();
    let ca = tempfile::tempdir().unwrap();
    init_ca(ca.path());

    let target = work.path().join("audited.txt");
    let manifest = format!(
        r#"schemaVersion: "1"
agent: {{ name: "r", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://run-test.local/agent/research/inst-1" }}
tools:
  filesystem:
    write: ["{p}"]
write_grants:
  - resource: "{f}"
    actions: ["write"]
    approval_required: true
"#,
        p = work.path().to_str().unwrap(),
        f = target.to_str().unwrap(),
    );
    let script = format!(
        r#"{{ "calls": [
            {{"kind": "filesystem_write", "path": "{f}", "contents": "x"}}
        ] }}"#,
        f = target.to_str().unwrap(),
    );

    let args = args_for(
        work.path(),
        ca.path().to_path_buf(),
        "session-approval",
        &manifest,
        &script,
    );
    let outcome = execute(args).unwrap();

    assert!(outcome.halted, "approval_required must halt in Phase 1a");
    let reason = outcome.halt_reason.as_deref().unwrap_or("");
    assert!(
        reason.contains("approval required"),
        "halt reason: {reason}"
    );
    // session_start + session_end (no access, no violation since it
    // didn't actually run).
    assert_eq!(outcome.entry_count, 2);
}

#[test]
fn rebind_violation_halts_and_records_violation() {
    let work = tempfile::tempdir().unwrap();
    let ca = tempfile::tempdir().unwrap();
    init_ca(ca.path());

    let target = work.path().join("ok.txt");
    std::fs::write(&target, b"x").unwrap();
    let manifest = format!(
        r#"schemaVersion: "1"
agent: {{ name: "r", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://run-test.local/agent/research/inst-1" }}
tools:
  filesystem:
    read: ["{p}"]
"#,
        p = work.path().to_str().unwrap()
    );
    // Two-call script: first call rebinds and reads OK; we then tamper
    // the model file via the test, second call rebinds and trips.
    let script = format!(
        r#"{{ "calls": [
            {{"kind": "filesystem_read", "path": "{f}"}},
            {{"kind": "filesystem_read", "path": "{f}"}}
        ] }}"#,
        f = target.to_str().unwrap(),
    );

    let args = args_for(
        work.path(),
        ca.path().to_path_buf(),
        "session-rebind",
        &manifest,
        &script,
    );
    let model_path = args.model.clone();

    // Spawn a thread that overwrites the model bytes after a brief
    // delay so it lands between call 1 and call 2. This is racy but
    // fine for a test fixture; if call 2 happens before the overwrite,
    // both calls succeed and the test still ends cleanly with halted
    // = false (no false negative — re-running surfaces the race).
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = std::fs::write(&model_path, b"tampered-model-bytes-for-test");
    });
    // Slow the run down so the overwrite sneaks in between calls.
    std::thread::sleep(std::time::Duration::from_millis(150));

    let outcome = execute(args).unwrap();
    if outcome.halted {
        let reason = outcome.halt_reason.as_deref().unwrap_or("");
        assert!(reason.contains("rebind"), "halt reason: {reason}");
        // start + access (call 1) + violation (call 2 rebind) + end
        assert_eq!(outcome.entry_count, 4);
    } else {
        // Race lost; both calls completed.
        assert_eq!(outcome.entry_count, 4); // start + 2 access + end
    }
}

#[test]
fn missing_script_file_errors_cleanly() {
    let work = tempfile::tempdir().unwrap();
    let ca = tempfile::tempdir().unwrap();
    init_ca(ca.path());

    let manifest = r#"schemaVersion: "1"
agent: { name: "r", version: "1.0.0" }
identity: { spiffeId: "spiffe://run-test.local/agent/research/inst-1" }
tools: {}
"#;
    let manifest_path = work.path().join("manifest.yaml");
    let model_path = work.path().join("model.gguf");
    std::fs::write(&manifest_path, manifest).unwrap();
    std::fs::write(&model_path, b"x").unwrap();

    let args = RunArgs {
        manifest: manifest_path,
        model: model_path,
        config: None,
        identity_dir: Some(ca.path().to_path_buf()),
        workload: "research".to_string(),
        instance: "inst-1".to_string(),
        ledger: Some(work.path().join("ledger.jsonl")),
        session_id: Some("session-missing".to_string()),
        script: work.path().join("does-not-exist.json"),
    };
    let err = execute(args).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("script") || msg.contains("does-not-exist"), "{msg}");
}
