//! Real-registry round-trip for `aegis pull`.
//!
//! Pulls the project-published Qwen2.5-1.5B-Instruct Q4_K_M OCI artifact
//! (per ADR-020 / ADR-021) through `pull::pull` against the live Sigstore
//! Rekor + Fulcio. Catches things the fake-tools tests in `tests/pull.rs`
//! can't:
//!
//! - real `oras` against GHCR (TLS, descriptor parsing, anonymous reads)
//! - real `cosign verify` against Sigstore's Fulcio cert + Rekor entry
//! - the actual OCI artifact layout `models-publish.yml` produces
//!
//! The test pins the specific digest from the workflow's first
//! successful run ([run 25135210278](https://github.com/tosin2013/aegis-node/actions/runs/25135210278))
//! rather than resolving `:latest` — so the assertion remains valid
//! regardless of future workflow re-runs that retag the same model
//! against a different revision.
//!
//! Skipped quietly when `oras` or `cosign` aren't on `$PATH`. CI's
//! `rust.yml` installs both before running this so every PR gets the
//! signal.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use aegis_cli::pull::{self, PullConfig};

/// Project-published Qwen2.5-1.5B-Instruct Q4_K_M OCI artifact pinned to
/// the digest produced by `models-publish.yml` run 25135210278. This is
/// the same value pinned in ADR-020 §"Pinned model" and the
/// SUPPLY_CHAIN.md smoke-test recipe.
///
/// `PINNED_REF` carries the **manifest digest** (per OCI spec — that's
/// what `<ref>@sha256:` always means). `PINNED_BLOB_SHA` is the actual
/// content hash of the GGUF bytes — what `pull::pull` returns and what
/// the F1 boot path will bind into the SVID. They are different values.
const PINNED_REF: &str = "ghcr.io/tosin2013/aegis-node-models/qwen2.5-1.5b-instruct-q4_k_m@sha256:240ece322070801d583241caaeced1a6b1ac55cbe42bf5379e95735ca89d4fa6";
const PINNED_MANIFEST_SHA: &str =
    "240ece322070801d583241caaeced1a6b1ac55cbe42bf5379e95735ca89d4fa6";
const PINNED_BLOB_SHA: &str = "6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e";

/// Identity regex matching the `models-publish.yml` workflow that signed
/// the artifact. Must stay in lockstep with the workflow file.
const MODELS_PUBLISH_IDENTITY_REGEXP: &str =
    r"^https://github\.com/tosin2013/aegis-node/\.github/workflows/models-publish\.yml@.*$";
const GH_ACTIONS_OIDC_ISSUER: &str = "https://token.actions.githubusercontent.com";

fn tool_on_path(tool: &str) -> bool {
    let path = match std::env::var_os("PATH") {
        Some(p) => p,
        None => return false,
    };
    std::env::split_paths(&path).any(|d| d.join(tool).is_file())
}

#[test]
fn pull_qwen_model_round_trips_against_real_registry() {
    if !tool_on_path("oras") || !tool_on_path("cosign") {
        eprintln!(
            "[skipped] oras and cosign must both be on $PATH for this test \
             (CI installs them; locally see docs/SUPPLY_CHAIN.md)"
        );
        return;
    }

    // Pull through the production code path against the live Sigstore
    // identity for `models-publish.yml`. Refusal at any gate (oras
    // failure, cosign mismatch, sha256 mismatch) panics with the
    // typed error so an operator sees exactly which step rejected.
    let cache = tempfile::tempdir().unwrap();
    let cfg = PullConfig {
        cache_dir: cache.path().to_path_buf(),
        cosign_key: None,
        keyless_identity: Some(MODELS_PUBLISH_IDENTITY_REGEXP.to_string()),
        keyless_oidc_issuer: Some(GH_ACTIONS_OIDC_ISSUER.to_string()),
    };
    let pulled = pull::pull(PINNED_REF, &cfg)
        .unwrap_or_else(|e| panic!("pull failed: {e}\nref: {PINNED_REF}"));

    // sha256 / cache layout / sidecar — same invariants pull::pull
    // documents publicly.
    assert_eq!(
        pulled.sha256_hex, PINNED_BLOB_SHA,
        "returned sha256 does not match the pinned digest"
    );
    assert!(pulled.blob_path.exists(), "blob not in cache");
    // Cache key is the *manifest* digest (per `pull::pull`'s public
    // contract), not the blob digest — the two are different OCI
    // concepts.
    assert_eq!(
        pulled.blob_path,
        cache_blob_path(cache.path(), PINNED_MANIFEST_SHA),
        "blob landed at unexpected cache path"
    );
    // sha256.txt sidecar carries the blob's actual SHA-256.
    let sha_sidecar = pulled.blob_path.parent().unwrap().join("sha256.txt");
    assert!(sha_sidecar.exists(), "sha256.txt sidecar missing");
    assert_eq!(
        std::fs::read_to_string(&sha_sidecar).unwrap().trim(),
        PINNED_BLOB_SHA,
        "sha256.txt should record the blob digest"
    );
    let ref_txt = pulled.blob_path.parent().unwrap().join("ref.txt");
    assert!(ref_txt.exists(), "ref.txt sidecar missing");
    let recorded = std::fs::read_to_string(&ref_txt).unwrap();
    assert_eq!(recorded, PINNED_REF, "ref.txt should record canonical ref");

    // The blob is a real GGUF — its first 4 bytes are the GGUF magic.
    // This is the strongest possible cross-check that pull::pull
    // delivered the file we think it did, end-to-end.
    let head = std::fs::read(&pulled.blob_path).unwrap();
    assert!(
        head.len() >= 4,
        "blob is impossibly small: {} bytes",
        head.len()
    );
    assert_eq!(
        &head[..4],
        b"GGUF",
        "first 4 bytes are not the GGUF magic — pulled artifact is not a GGUF"
    );
}

fn cache_blob_path(cache: &std::path::Path, sha_hex: &str) -> PathBuf {
    cache.join(sha_hex).join("blob.bin")
}
