//! End-to-end runtime conformance test (issue #29, F0-F).
//!
//! Runs `aegis run` against a templated fixture in `tests/runtime/`,
//! normalizes non-deterministic fields (entryId / timestamp / prevHash /
//! digest hexes / agentIdentityHash) in both the produced ledger and
//! the golden, then asserts entry-by-entry equality. Also calls
//! `aegis verify` to confirm the chain integrity.
//!
//! This is the capstone test for v0.5.0: a fixed input produces a
//! deterministic ledger, and the runtime + writer + verifier all agree
//! on what that ledger should look like.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use aegis_cli::run::{execute, RunArgs};
use aegis_identity::LocalCa;
use aegis_ledger_writer::verify_file;
use serde_json::Value;

const TRUST_DOMAIN: &str = "runtime-conformance.local";
const SESSION_ID: &str = "session-runtime-conformance";

const FIXTURE_DIR: &str = "../../tests/runtime";
const MODEL_BYTES: &[u8] = b"runtime-conformance-fixture-model-bytes-v1\n";
const INPUT_BYTES: &[u8] = b"hello";
const SCRATCH_BYTES: &[u8] = b"x";

/// Fields whose values are not deterministic across runs and must be
/// elided before comparing the produced ledger to the golden.
fn placeholder_for(field: &str) -> Option<&'static str> {
    Some(match field {
        "entryId" => "<UUID>",
        "timestamp" => "<TIME>",
        "prevHash" => "<HEX64>",
        "modelDigestHex" => "<HEX64>",
        "manifestDigestHex" => "<HEX64>",
        "configDigestHex" => "<HEX64>",
        "agentIdentityHash" => "<HEX64>",
        _ => return None,
    })
}

fn normalize(mut entry: Value) -> Value {
    if let Some(obj) = entry.as_object_mut() {
        let keys: BTreeSet<String> = obj.keys().cloned().collect();
        for k in keys {
            if let Some(p) = placeholder_for(&k) {
                obj.insert(k, Value::String(p.to_string()));
            }
        }
    }
    entry
}

fn read_jsonl(path: &Path) -> Vec<Value> {
    let s = std::fs::read_to_string(path).unwrap_or_else(|e| {
        panic!("read {}: {e}", path.display());
    });
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("parse line: {e}: {l}")))
        .collect()
}

fn substitute(template: &str, workdir: &Path) -> String {
    template.replace("{{WORKDIR}}", workdir.to_str().unwrap())
}

fn fixture_path(file: &str) -> PathBuf {
    Path::new(FIXTURE_DIR).join(file)
}

#[test]
fn runtime_conformance_golden() {
    let work = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    LocalCa::init(ca_dir.path(), TRUST_DOMAIN).unwrap();

    // Set up the workdir mirror of the fixture script's expectations.
    std::fs::create_dir_all(work.path().join("inputs")).unwrap();
    std::fs::create_dir_all(work.path().join("outputs")).unwrap();
    std::fs::create_dir_all(work.path().join("forbidden")).unwrap();
    std::fs::write(work.path().join("inputs/in.txt"), INPUT_BYTES).unwrap();
    std::fs::write(work.path().join("outputs/scratch.txt"), SCRATCH_BYTES).unwrap();
    std::fs::write(work.path().join("forbidden/secret.txt"), b"secret").unwrap();
    std::fs::write(work.path().join("model.gguf"), MODEL_BYTES).unwrap();

    // Substitute templates → concrete files under the workdir.
    let manifest_path = work.path().join("manifest.yaml");
    let script_path = work.path().join("script.json");
    let manifest_template = std::fs::read_to_string(fixture_path("manifest.template.yaml"))
        .expect("read manifest template");
    let script_template = std::fs::read_to_string(fixture_path("script.template.json"))
        .expect("read script template");
    std::fs::write(&manifest_path, substitute(&manifest_template, work.path())).unwrap();
    std::fs::write(&script_path, substitute(&script_template, work.path())).unwrap();

    let ledger_path = work.path().join("ledger.jsonl");
    let args = RunArgs {
        manifest: manifest_path,
        model: work.path().join("model.gguf"),
        config: None,
        identity_dir: Some(ca_dir.path().to_path_buf()),
        workload: "conformance".to_string(),
        instance: "inst-1".to_string(),
        ledger: Some(ledger_path.clone()),
        session_id: Some(SESSION_ID.to_string()),
        script: script_path,
    };

    let outcome = execute(args).expect("aegis run");
    assert!(!outcome.halted, "halt_reason: {:?}", outcome.halt_reason);

    // Chain integrity check.
    let summary = verify_file(&ledger_path).expect("verify_file ok");
    assert_eq!(summary.entry_count, outcome.entry_count);

    // Normalize produced + load golden with workdir substitution.
    let produced: Vec<Value> = read_jsonl(&ledger_path).into_iter().map(normalize).collect();

    let golden_template = std::fs::read_to_string(fixture_path("golden.template.jsonl"))
        .expect("read golden template");
    let golden_substituted = substitute(&golden_template, work.path());
    let golden: Vec<Value> = golden_substituted
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("parse golden: {e}: {l}")))
        .map(normalize)
        .collect();

    assert_eq!(
        produced.len(),
        golden.len(),
        "entry count drift: produced={}, golden={}\nproduced (normalized):\n{}",
        produced.len(),
        golden.len(),
        format_entries(&produced),
    );

    for (i, (p, g)) in produced.iter().zip(golden.iter()).enumerate() {
        if p != g {
            panic!(
                "entry {i} drift\nproduced:\n{}\ngolden:\n{}",
                serde_json::to_string_pretty(p).unwrap(),
                serde_json::to_string_pretty(g).unwrap(),
            );
        }
    }
}

fn format_entries(entries: &[Value]) -> String {
    entries
        .iter()
        .enumerate()
        .map(|(i, e)| format!("[{i}] {}", serde_json::to_string(e).unwrap()))
        .collect::<Vec<_>>()
        .join("\n")
}
