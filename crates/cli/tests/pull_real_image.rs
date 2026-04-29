//! Real-registry round-trip for `aegis pull`.
//!
//! Pulls the Aegis-Node devbox image from `ghcr.io/tosin2013/aegis-node-devbox`
//! (the one signed artifact we publish today, per ADR-017) through the same
//! `pull::pull` code path operators use. Catches things the fake-tools test
//! in `tests/pull.rs` can't:
//!
//! - real `oras` against a real registry (TLS, descriptor parsing, auth-anonymous)
//! - real `cosign verify` against the live Sigstore Rekor + Fulcio
//! - GHCR's actual response shapes
//!
//! Skipped quietly when `oras` or `cosign` aren't on `$PATH`. The CI job in
//! `.github/workflows/rust.yml` installs both before running this so PRs see
//! signal; local developers without them get the fake-tools test as their
//! inner-loop coverage and the `[skipped]` line as a hint to install.
//!
//! Once we publish the demo-program model artifact (ADR-021 / Qwen2.5-1.5B
//! mirror), we can add a sibling test that pulls *that* and assert the
//! resulting blob starts with the GGUF magic. For now the devbox image is
//! the only signed artifact available.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

use aegis_cli::pull::{self, PullConfig};

const DEVBOX_REPO: &str = "ghcr.io/tosin2013/aegis-node-devbox";

/// SPIFFE-shaped identity regex matching the devbox publisher workflow.
/// Same value documented in `docs/SUPPLY_CHAIN.md`.
const DEVBOX_IDENTITY_REGEXP: &str =
    r"^https://github\.com/tosin2013/aegis-node/\.github/workflows/devbox\.yml@.*$";
const GH_ACTIONS_OIDC_ISSUER: &str = "https://token.actions.githubusercontent.com";

fn tool_on_path(tool: &str) -> bool {
    let path = match std::env::var_os("PATH") {
        Some(p) => p,
        None => return false,
    };
    std::env::split_paths(&path).any(|d| d.join(tool).is_file())
}

#[test]
fn pull_devbox_image_round_trips_against_real_registry() {
    if !tool_on_path("oras") || !tool_on_path("cosign") {
        eprintln!(
            "[skipped] oras and cosign must both be on $PATH for this test \
             (CI installs them; locally see docs/SUPPLY_CHAIN.md)"
        );
        return;
    }

    // 1. Resolve the live :latest digest. Pinning by digest is mandatory
    //    in `pull::pull` (see ADR-013 / OCI-A) — we resolve it via oras,
    //    not by hardcoding, so the test stays valid as the devbox is
    //    rebuilt.
    let descriptor = Command::new("oras")
        .args(["manifest", "fetch", "--descriptor"])
        .arg(format!("{DEVBOX_REPO}:latest"))
        .output()
        .expect("oras manifest fetch failed to spawn");
    if !descriptor.status.success() {
        panic!(
            "oras manifest fetch exited {:?}: stderr={}",
            descriptor.status.code(),
            String::from_utf8_lossy(&descriptor.stderr),
        );
    }
    let descriptor_json: serde_json::Value =
        serde_json::from_slice(&descriptor.stdout).expect("oras descriptor not valid JSON");
    let digest_field = descriptor_json
        .get("digest")
        .and_then(|v| v.as_str())
        .expect("descriptor missing digest");
    let sha_hex = digest_field
        .strip_prefix("sha256:")
        .expect("descriptor digest must use the sha256: prefix");

    let pinned_ref = format!("{DEVBOX_REPO}@sha256:{sha_hex}");

    // 2. Pull through the production code path, against the live
    //    Sigstore identity for the devbox publisher workflow.
    let cache = tempfile::tempdir().unwrap();
    let cfg = PullConfig {
        cache_dir: cache.path().to_path_buf(),
        cosign_key: None,
        keyless_identity: Some(DEVBOX_IDENTITY_REGEXP.to_string()),
        keyless_oidc_issuer: Some(GH_ACTIONS_OIDC_ISSUER.to_string()),
    };
    let pulled = pull::pull(&pinned_ref, &cfg)
        .unwrap_or_else(|e| panic!("pull failed: {e}\nref: {pinned_ref}"));

    // 3. Sanity: returned sha256 matches the descriptor; cache layout
    //    is the documented `<cache-dir>/<sha256>/blob.bin`.
    assert_eq!(
        pulled.sha256_hex, sha_hex,
        "returned sha256 does not match the descriptor"
    );
    assert!(pulled.blob_path.exists(), "blob not in cache");
    assert_eq!(
        pulled.blob_path,
        cache_blob_path(cache.path(), sha_hex),
        "blob landed at unexpected cache path"
    );
    let ref_txt = pulled.blob_path.parent().unwrap().join("ref.txt");
    assert!(ref_txt.exists(), "ref.txt sidecar missing");
    let recorded = std::fs::read_to_string(&ref_txt).unwrap();
    assert_eq!(recorded, pinned_ref, "ref.txt should record canonical ref");
}

fn cache_blob_path(cache: &std::path::Path, sha_hex: &str) -> PathBuf {
    cache.join(sha_hex).join("blob.bin")
}
