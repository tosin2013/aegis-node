//! End-to-end smoke test: load a real `.litertlm`, infer one turn.
//!
//! Per LiteRT-A acceptance criteria: "load `gemma-4-E2B-it-litert-lm`
//! from a local path and complete one greedy-sampled turn." The
//! fixture is multi-gigabyte and too heavy to pull on every PR, so
//! this test is `#[ignore]` by default and reads its model path from
//! the `AEGIS_LITERTLM_TEST_MODEL` environment variable — same shape
//! as `aegis-llama-backend`'s smoke test.
//!
//! Local invocation (once Gemma 4 is published via LiteRT-C):
//!
//! ```bash
//! aegis pull \
//!   ghcr.io/tosin2013/aegis-node-models/gemma-3n-e2b-it-litertlm@sha256:... \
//!   --keyless-identity '...models-publish.yml@.*' \
//!   --keyless-oidc-issuer 'https://token.actions.githubusercontent.com'
//!
//! AEGIS_LITERTLM_TEST_MODEL=~/.cache/aegis/models/.../blob.bin \
//!   LITERT_LM_PREBUILT_SO=/staging/libaegis_litertlm_engine_cpu.so \
//!   cargo test -p aegis-litertlm-backend -- --ignored --include-ignored smoke
//! ```
//!
//! CI integration lands as a dedicated `litertlm.yml` path-filtered
//! job alongside the existing `llama.yml` (LiteRT-A's
//! workspace+CI-wiring task).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use aegis_litertlm_backend::{
    set_min_log_level, DeterminismKnobs, Engine, LiteRtError, Session, SessionOptions,
};

const ENV_KEY: &str = "AEGIS_LITERTLM_TEST_MODEL";

fn fixture_path() -> Option<PathBuf> {
    std::env::var_os(ENV_KEY).map(PathBuf::from)
}

#[test]
fn missing_model_file_returns_typed_error() {
    let err = Engine::load(std::path::Path::new("/no/such/path.litertlm")).unwrap_err();
    assert!(
        matches!(err, LiteRtError::ModelFileUnreadable { .. }),
        "got {err:?}"
    );
}

#[test]
#[ignore = "loads a real .litertlm; set AEGIS_LITERTLM_TEST_MODEL=<path>"]
fn smoke_load_and_infer_one_turn() {
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

    // Surface upstream's INFO logs so a NULL return from
    // engine_create / session_create has a corresponding upstream
    // log line for diagnosis.
    set_min_log_level(0);

    let engine = Engine::load(&path).expect("engine load");
    let mut session = Session::new(
        &engine,
        SessionOptions {
            // Keep this short so the smoke test stays under a minute
            // even on a small CPU.
            max_tokens: 32,
            determinism: DeterminismKnobs::default(),
        },
    )
    .expect("session new");

    let out = session
        .infer("The capital of France is")
        .expect("inference completes");

    assert!(!out.is_empty(), "model produced empty output");
    eprintln!("[smoke] model returned: {out:?}");
}

#[test]
#[ignore = "loads a real .litertlm; set AEGIS_LITERTLM_TEST_MODEL=<path>"]
fn determinism_seed_yields_byte_identical_output_across_two_runs() {
    // Phase 1 is CPU + greedy per ADR-023. Greedy = argmax = fully
    // deterministic regardless of seed. Two engine+session pairs over
    // the same prompt + same configuration must produce identical
    // text. (GPU determinism is broken upstream — google-ai-edge/LiteRT-LM
    // #2080 / #2081 — and out of scope for Phase 1.)
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

    set_min_log_level(0);

    let opts = || SessionOptions {
        max_tokens: 24,
        determinism: DeterminismKnobs {
            seed: Some(42),
            temperature: Some(0.0),
            top_p: Some(1.0),
            top_k: Some(0),
            repeat_penalty: Some(1.0),
        },
    };

    let prompt = "Once upon a time,";

    // Reload the engine for each run — this exercises the full
    // engine_create + session_create path, not just session reuse.
    let engine1 = Engine::load(&path).expect("engine 1 load");
    let mut s1 = Session::new(&engine1, opts()).expect("session 1");
    let out1 = s1.infer(prompt).expect("infer 1");

    let engine2 = Engine::load(&path).expect("engine 2 load");
    let mut s2 = Session::new(&engine2, opts()).expect("session 2");
    let out2 = s2.infer(prompt).expect("infer 2");

    eprintln!("[determinism] run 1: {out1:?}");
    eprintln!("[determinism] run 2: {out2:?}");
    assert_eq!(
        out1, out2,
        "same prompt + seed + temperature=0 (greedy) must produce identical output"
    );
}
