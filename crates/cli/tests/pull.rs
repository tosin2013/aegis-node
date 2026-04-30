//! End-to-end orchestration tests for `aegis pull`.
//!
//! Per OCI-A / ADR-013 / issue #66. Real registries / cosign keys
//! aren't reachable in CI — instead we drop fake `oras` and `cosign`
//! shell scripts on PATH that mimic their interface (oras writes a
//! known blob to its `-o` dir; cosign exits 0 for "verified" or 1
//! for "refused"). That way the test exercises every gate in
//! `pull::pull` (ref parse → oras invocation → cosign verify →
//! sha256 recompute → cache move) without depending on the network.
//!
//! A real-registry round-trip lives in docs/SUPPLY_CHAIN.md as the
//! operator workflow. F1 boot-path integration ("session boots
//! against a pulled blob and the SVID's bound model digest matches")
//! lands with the F1 wiring follow-up — out of scope for OCI-A.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use aegis_cli::pull::{self, PullConfig, PullError};
use sha2::{Digest, Sha256};

/// Build a fake `oras` script that writes `blob_bytes` to its -o dir
/// and a fake `cosign` script that exits with `cosign_exit_code`.
///
/// Returns a directory we can prepend to PATH.
fn fake_tool_dir(blob_bytes: &[u8], blob_name: &str, cosign_exit_code: i32) -> PathBuf {
    let dir = tempfile::tempdir().unwrap().keep();
    // Encode the blob into the script as a here-doc base64 wouldn't
    // round-trip binary cleanly, so we drop the bytes to a sidecar
    // file the script copies on every invocation. This keeps the
    // script tiny and binary-clean.
    let blob_src = dir.join("__blob.bin");
    fs::write(&blob_src, blob_bytes).unwrap();

    let oras_path = dir.join("oras");
    let oras = format!(
        r#"#!/usr/bin/env bash
# fake oras: usage `oras pull -o <out_dir> <ref>` — copy the canned
# blob into <out_dir>/<blob_name>.
set -euo pipefail
out_dir=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    -o) out_dir="$2"; shift 2 ;;
    *)  shift ;;
  esac
done
if [[ -z "$out_dir" ]]; then
  echo "fake oras: missing -o" >&2
  exit 2
fi
mkdir -p "$out_dir"
cp "{src}" "$out_dir/{blob_name}"
"#,
        src = blob_src.display(),
        blob_name = blob_name,
    );
    fs::write(&oras_path, oras).unwrap();
    chmod_exec(&oras_path);

    let cosign_path = dir.join("cosign");
    let cosign = format!(
        r#"#!/usr/bin/env bash
# fake cosign: exit `{exit_code}` to model "verified" / "refused".
exit {exit_code}
"#,
        exit_code = cosign_exit_code,
    );
    fs::write(&cosign_path, cosign).unwrap();
    chmod_exec(&cosign_path);

    dir
}

fn chmod_exec(p: &Path) {
    let mut perm = fs::metadata(p).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(p, perm).unwrap();
}

/// Prepend `dir` to PATH for the duration of the test, restoring it
/// at scope end. Tests serialize on PATH via `tempfile::env_lock` —
/// without that they clobber each other in parallel runs.
struct PathGuard {
    original: Option<std::ffi::OsString>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl PathGuard {
    fn prepend(dir: &Path) -> Self {
        // Serialize PATH mutations across tests in this binary.
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        // poison-resistant lock acquisition.
        let lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original = std::env::var_os("PATH");
        let mut new = std::ffi::OsString::from(dir);
        if let Some(p) = &original {
            new.push(":");
            new.push(p);
        }
        std::env::set_var("PATH", &new);
        Self {
            original,
            _lock: lock,
        }
    }
}
impl Drop for PathGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }
    }
}

fn cfg(cache_dir: PathBuf) -> PullConfig {
    PullConfig {
        cache_dir,
        cosign_key: None,
        keyless_identity: None,
        keyless_oidc_issuer: None,
    }
}

#[test]
fn pull_succeeds_and_returns_blob_sha256() {
    let blob = b"fake gguf bytes for a tiny model";
    let mut hasher = Sha256::new();
    hasher.update(blob);
    let blob_sha = hex::encode(hasher.finalize());
    // Note: in real OCI the ref's `@sha256:` is the *manifest* digest,
    // not the blob digest. The fake-tools world has no manifest layer,
    // so we put any 64-char hex here — it just becomes the cache-key.
    let manifest_sha = "1".repeat(64);
    let reference = format!("ghcr.io/example/tiny-model@sha256:{manifest_sha}");

    let tools = fake_tool_dir(blob, "model.gguf", 0);
    let _path = PathGuard::prepend(&tools);

    let cache = tempfile::tempdir().unwrap();
    let pulled = pull::pull(&reference, &cfg(cache.path().to_path_buf())).unwrap();

    // pull::pull surfaces the *blob* SHA-256 (what F1 binds into the
    // SVID), not the ref's manifest digest.
    assert_eq!(pulled.sha256_hex, blob_sha);
    assert!(pulled.blob_path.exists(), "blob not in cache");
    let actual_bytes = fs::read(&pulled.blob_path).unwrap();
    assert_eq!(actual_bytes, blob);
    // Cache layout: <cache>/<manifest-sha>/blob.bin + ref.txt + sha256.txt.
    let dir = pulled.blob_path.parent().unwrap();
    assert!(
        dir.ends_with(&manifest_sha),
        "cache key should be manifest sha"
    );
    assert!(dir.join("ref.txt").exists());
    let recorded = fs::read_to_string(dir.join("sha256.txt")).unwrap();
    assert_eq!(recorded.trim(), blob_sha);
}

#[test]
fn pull_refuses_when_cosign_verify_fails() {
    let blob = b"some bytes";
    let manifest_sha = "2".repeat(64);
    let reference = format!("ghcr.io/example/tiny-model@sha256:{manifest_sha}");

    // cosign exits 1 → CosignVerifyFailed.
    let tools = fake_tool_dir(blob, "model.gguf", 1);
    let _path = PathGuard::prepend(&tools);

    let cache = tempfile::tempdir().unwrap();
    let err = pull::pull(&reference, &cfg(cache.path().to_path_buf())).unwrap_err();
    match err {
        PullError::CosignVerifyFailed { exit_code, .. } => {
            assert_eq!(exit_code, Some(1));
        }
        other => panic!("expected CosignVerifyFailed, got {other:?}"),
    }
    assert!(
        !cache.path().join(manifest_sha).exists(),
        "cosign-failed artifact must NOT be cached"
    );
}

#[test]
fn pull_short_circuits_when_blob_already_cached() {
    let blob = b"fake gguf bytes for a tiny model";
    let mut hasher = Sha256::new();
    hasher.update(blob);
    let blob_sha = hex::encode(hasher.finalize());
    let manifest_sha = "3".repeat(64);
    let reference = format!("ghcr.io/example/tiny-model@sha256:{manifest_sha}");

    let tools = fake_tool_dir(blob, "model.gguf", 0);
    let _path = PathGuard::prepend(&tools);

    let cache = tempfile::tempdir().unwrap();
    // First pull populates the cache.
    let _ = pull::pull(&reference, &cfg(cache.path().to_path_buf())).unwrap();

    // Corrupt the oras script *in place* — the second pull must not
    // invoke it because the blob is already cached. Rewriting the
    // existing script keeps a single PathGuard alive (a second guard
    // in the same scope would deadlock — std::sync::Mutex isn't
    // reentrant).
    fs::write(tools.join("oras"), "#!/bin/sh\nexit 99\n").unwrap();
    chmod_exec(&tools.join("oras"));

    let pulled = pull::pull(&reference, &cfg(cache.path().to_path_buf())).unwrap();
    assert_eq!(pulled.sha256_hex, blob_sha);
}

#[test]
fn pull_refuses_when_cached_blob_corrupted() {
    let blob = b"fake gguf bytes for a tiny model";
    let manifest_sha = "4".repeat(64);
    let reference = format!("ghcr.io/example/tiny-model@sha256:{manifest_sha}");

    let tools = fake_tool_dir(blob, "model.gguf", 0);
    let _path = PathGuard::prepend(&tools);

    let cache = tempfile::tempdir().unwrap();
    let pulled = pull::pull(&reference, &cfg(cache.path().to_path_buf())).unwrap();

    // Corrupt the cached blob — next pull MUST refuse, not silently
    // hand back the bad bytes.
    fs::write(&pulled.blob_path, b"tampered").unwrap();

    let err = pull::pull(&reference, &cfg(cache.path().to_path_buf())).unwrap_err();
    assert!(matches!(err, PullError::Sha256Mismatch { .. }), "{err}");
}
