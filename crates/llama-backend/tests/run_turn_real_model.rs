//! End-to-end real-model test: LlamaCppBackend + Session::run_turn
//! against a real GGUF, gated by `AEGIS_LLAMA_TEST_MODEL`.
//!
//! Mirrors the LLM-A smoke test's gating: `#[ignore]` by default,
//! reads the model path from the env var. Validates the full chain
//! (boot → backend.load → run_turn → infer → reasoning emission) end
//! to end on a real model — the path that matters for Phase 2 demo
//! recordings.
//!
//! Local invocation:
//!
//! ```bash
//! AEGIS_LLAMA_TEST_MODEL=~/.cache/aegis/models/<sha>/blob.bin \
//!   cargo test -p aegis-llama-backend --test run_turn_real_model -- --ignored
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::Arc;

use aegis_inference_engine::{Backend as RuntimeBackend, BootConfig, Session};
use aegis_llama_backend::{Backend, DeterminismKnobs, LlamaCppBackend, SessionOptions};

const ENV_KEY: &str = "AEGIS_LLAMA_TEST_MODEL";

fn fixture_path() -> Option<PathBuf> {
    std::env::var_os(ENV_KEY).map(PathBuf::from)
}

#[test]
#[ignore = "loads a real GGUF + boots a Session; set AEGIS_LLAMA_TEST_MODEL=<path>"]
fn llama_cpp_backend_round_trips_a_run_turn_call() {
    use aegis_identity::LocalCa;

    let path = match fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("[skipped] {ENV_KEY} not set");
            return;
        }
    };
    if !path.exists() {
        eprintln!("[skipped] {} does not exist", path.display());
        return;
    }

    // Stand up the FFI backend + LLM-B wrapper.
    let llama = Arc::new(Backend::init().expect("backend init"));
    let cpp_backend = LlamaCppBackend::new(
        llama.clone(),
        SessionOptions {
            n_ctx: 1024,
            max_tokens: 32,
            determinism: DeterminismKnobs::default(),
        },
    );
    let model = cpp_backend.load(&path).expect("load");

    // Boot a Session with the same fixture as a model digest source.
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    LocalCa::init(ca_dir.path(), "session-boot.local").unwrap();

    let manifest_path = dir.path().join("manifest.yaml");
    std::fs::write(
        &manifest_path,
        r#"schemaVersion: "1"
agent: { name: "real-model-turn", version: "1.0.0" }
identity: { spiffeId: "spiffe://session-boot.local/agent/research/inst-001" }
tools: {}
"#,
    )
    .unwrap();

    let cfg = BootConfig {
        session_id: "session-real-turn".to_string(),
        manifest_path,
        // The test deliberately uses the same GGUF as the digest
        // source — the wrapper hashes it via sha256_file, doesn't
        // actually load it for inference (the LlamaCppBackend does).
        model_path: path.clone(),
        config_path: None,
        chat_template_sidecar: None,
        identity_dir: ca_dir.path().to_path_buf(),
        workload_name: "research".to_string(),
        instance: "inst-001".to_string(),
        ledger_path: dir.path().join("ledger.jsonl"),
    };
    let session = Session::boot(cfg).expect("boot");
    let mut session = session.with_loaded_model(model);

    let outcome = session
        .run_turn("In one short word, what is the capital of France?")
        .expect("run_turn");

    // Most modern instruct models will produce text on this prompt.
    // Tool calls are zero (tools={}); we just check the reasoning
    // wasn't empty.
    eprintln!("[real-model] outcome: {outcome:?}");
    assert!(
        outcome.assistant_text.is_some() || !outcome.tool_calls.is_empty(),
        "real-model run_turn produced neither reasoning nor tool calls: {outcome:?}"
    );

    let _ = session.shutdown().expect("shutdown");
    let ledger = std::fs::read_to_string(dir.path().join("ledger.jsonl")).unwrap();
    assert!(
        ledger.contains("reasoning_step"),
        "ledger should carry an F5 ReasoningStep entry: {ledger}"
    );
}
