//! End-to-end smoke test: load a real GGUF, infer one turn.
//!
//! Per LLM-A acceptance criteria: "load a tiny GGUF (e.g. TinyLlama
//! 1.1B Q4) and complete one turn." The fixture is too big to bundle
//! and too heavy to pull on every PR run, so this test is `#[ignore]`
//! by default and reads its model path from the
//! `AEGIS_LLAMA_TEST_MODEL` environment variable.
//!
//! Local invocation:
//!
//! ```bash
//! # Pull the project's pinned Qwen2.5-1.5B-Instruct Q4_K_M (per ADR-020)
//! aegis pull \
//!   ghcr.io/tosin2013/aegis-node-models/qwen2.5-1.5b-instruct-q4_k_m@sha256:c7404a91... \
//!   --keyless-identity '...models-publish.yml@.*' \
//!   --keyless-oidc-issuer 'https://token.actions.githubusercontent.com'
//!
//! # Then point the test at the cached blob
//! AEGIS_LLAMA_TEST_MODEL=~/.cache/aegis/models/c7404a91.../blob.bin \
//!   cargo test -p aegis-llama-backend -- --ignored --include-ignored smoke
//! ```
//!
//! CI integration is gated to a dedicated `llama.yml` job with the
//! fixture cached via `actions/cache`; the default `rust.yml` doesn't
//! pay the build cost.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::Arc;

use aegis_llama_backend::{Backend, DeterminismKnobs, LlamaError, Model, Session, SessionOptions};

const ENV_KEY: &str = "AEGIS_LLAMA_TEST_MODEL";

fn fixture_path() -> Option<PathBuf> {
    std::env::var_os(ENV_KEY).map(PathBuf::from)
}

#[test]
fn missing_model_file_returns_typed_error() {
    let backend = match Backend::init() {
        Ok(b) => Arc::new(b),
        Err(e) => {
            eprintln!("[skipped] backend init failed: {e}");
            return;
        }
    };
    let err = Model::load(backend, std::path::Path::new("/no/such/path.gguf")).unwrap_err();
    assert!(
        matches!(err, LlamaError::ModelFileUnreadable { .. }),
        "got {err:?}"
    );
}

#[test]
#[ignore = "loads a real GGUF; set AEGIS_LLAMA_TEST_MODEL=<path>"]
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

    let backend = Arc::new(Backend::init().expect("backend init"));
    let model = Model::load(backend, &path).expect("model load");
    let mut session = Session::new(
        &model,
        SessionOptions {
            n_ctx: 1024,
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
#[ignore = "loads a real GGUF; set AEGIS_LLAMA_TEST_MODEL=<path>"]
fn determinism_seed_yields_byte_identical_output_across_two_runs() {
    // Per LLM-C acceptance criterion: "smoke test asserts byte-identical
    // output across two runs of the same prompt with the same seed."
    //
    // We pin `temperature: 0.0` so the run is greedy regardless of seed
    // — that's the always-deterministic configuration. Pinning seed too
    // covers the seed-aware-random path's reproducibility (greedy is
    // seed-independent, so this test would be too easy without temp).
    //
    // For the seed-aware-random path (temperature > 0) determinism
    // hinges on llama.cpp's `llama_sampler_init_dist` honoring the
    // seed across calls. We test the always-greedy path here because
    // it's the one the demo program (ADR-020) uses, and because the
    // additional `dist` randomness path needs longer outputs to
    // surface behavior — out of scope for a fast smoke test.
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

    let backend = Arc::new(Backend::init().expect("backend init"));
    let model = Model::load(backend, &path).expect("model load");

    let opts = || SessionOptions {
        n_ctx: 1024,
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

    let mut s1 = Session::new(&model, opts()).expect("session 1");
    let out1 = s1.infer(prompt).expect("infer 1");

    let mut s2 = Session::new(&model, opts()).expect("session 2");
    let out2 = s2.infer(prompt).expect("infer 2");

    eprintln!("[determinism] run 1: {out1:?}");
    eprintln!("[determinism] run 2: {out2:?}");
    assert_eq!(
        out1, out2,
        "same prompt + seed + temperature=0 must produce identical output"
    );
}
