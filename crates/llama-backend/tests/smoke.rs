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

use aegis_llama_backend::{Backend, LlamaError, Model, Session, SessionOptions};

const ENV_KEY: &str = "AEGIS_LLAMA_TEST_MODEL";

fn fixture_path() -> Option<PathBuf> {
    std::env::var_os(ENV_KEY).map(PathBuf::from)
}

#[test]
fn missing_model_file_returns_typed_error() {
    let backend = match Backend::init() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[skipped] backend init failed: {e}");
            return;
        }
    };
    let err = Model::load(&backend, std::path::Path::new("/no/such/path.gguf")).unwrap_err();
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

    let backend = Backend::init().expect("backend init");
    let model = Model::load(&backend, &path).expect("model load");
    let mut session = Session::new(
        &model,
        SessionOptions {
            n_ctx: 1024,
            // Keep this short so the smoke test stays under a minute
            // even on a small CPU.
            max_tokens: 32,
        },
    )
    .expect("session new");

    let out = session
        .infer("The capital of France is")
        .expect("inference completes");

    assert!(!out.is_empty(), "model produced empty output");
    eprintln!("[smoke] model returned: {out:?}");
}
