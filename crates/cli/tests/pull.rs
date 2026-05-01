//! End-to-end orchestration tests for `aegis pull`.
//!
//! Per OCI-A / OCI-B / ADR-013 / ADR-022 / issues #66 and #67. Real
//! registries / cosign keys aren't reachable in CI — instead we drop
//! fake `oras` and `cosign` shell scripts on PATH that mimic their
//! interface:
//!
//! - fake `oras pull` writes a known blob to its `-o` dir
//! - fake `oras manifest fetch` echoes a fixture JSON manifest
//! - fake `cosign verify` exits 0 for "verified" or 1 for "refused"
//!
//! That way the test exercises every gate in `pull::pull` (ref parse →
//! manifest fetch → annotation extraction → oras pull → cosign verify →
//! sha256 recompute → cache move) without depending on the network.
//!
//! A real-registry round-trip lives in `tests/pull_real_image.rs` and
//! runs against the live Qwen artifact `models-publish.yml` produces.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use aegis_cli::pull::{self, PullConfig, PullError};
use sha2::{Digest, Sha256};

const MODEL_GGUF_MEDIA_TYPE: &str = "application/vnd.aegis-node.model.gguf.v1";
const MODEL_LITERTLM_MEDIA_TYPE: &str = "application/vnd.aegis-node.model.litertlm.v1";
const NON_MODEL_MEDIA_TYPE: &str = "application/vnd.example.unknown.v1";

/// Build a synthetic OCI manifest JSON with the given artifact-type and
/// optional chat-template annotation. Mirrors what
/// `oras manifest fetch` returns from a real registry.
fn manifest_json(artifact_type: &str, chat_template_sha: Option<&str>) -> String {
    let annotations = match chat_template_sha {
        Some(s) => format!(r#""annotations":{{"dev.aegis-node.chat-template.sha256":"{s}"}},"#),
        None => String::new(),
    };
    format!(
        r#"{{
            "schemaVersion":2,
            "mediaType":"application/vnd.oci.image.manifest.v1+json",
            "artifactType":"{artifact_type}",
            "config":{{"mediaType":"application/vnd.oci.empty.v1+json","digest":"sha256:0000","size":0}},
            "layers":[{{"mediaType":"{artifact_type}","digest":"sha256:1111","size":42}}],
            {annotations}
            "_test":"synthetic"
        }}"#
    )
}

/// Build fake `oras` + `cosign` scripts on a temp PATH dir.
///
/// `oras pull -o <dir> <ref>` copies a canned blob to `<dir>/<blob_name>`.
/// `oras manifest fetch <ref>` echoes `manifest_json` on stdout.
/// `cosign verify ...` exits with `cosign_exit_code`.
fn fake_tool_dir(
    blob_bytes: &[u8],
    blob_name: &str,
    cosign_exit_code: i32,
    manifest_json: &str,
) -> PathBuf {
    let dir = tempfile::tempdir().unwrap().keep();

    // Drop the blob bytes to a sidecar — easier than encoding them into
    // a shell here-doc, and keeps binary content clean.
    let blob_src = dir.join("__blob.bin");
    fs::write(&blob_src, blob_bytes).unwrap();

    // The fake `oras` writes the manifest JSON via `cat` of a sidecar
    // file. That keeps escaping out of the shell heredoc — the JSON has
    // braces and colons that bash gets fussy about.
    let manifest_src = dir.join("__manifest.json");
    fs::write(&manifest_src, manifest_json).unwrap();

    let oras_path = dir.join("oras");
    let oras = format!(
        r#"#!/usr/bin/env bash
# fake oras: dispatches `pull` and `manifest fetch` subcommands.
set -euo pipefail
sub="${{1:-}}"
case "$sub" in
  pull)
    shift
    out_dir=""
    while [[ $# -gt 0 ]]; do
      case "$1" in
        -o) out_dir="$2"; shift 2 ;;
        *)  shift ;;
      esac
    done
    if [[ -z "$out_dir" ]]; then
      echo "fake oras pull: missing -o" >&2
      exit 2
    fi
    mkdir -p "$out_dir"
    cp "{blob_src}" "$out_dir/{blob_name}"
    ;;
  manifest)
    shift
    if [[ "${{1:-}}" != "fetch" ]]; then
      echo "fake oras manifest: only 'fetch' supported" >&2
      exit 2
    fi
    cat "{manifest_src}"
    ;;
  *)
    echo "fake oras: unknown subcommand '$sub'" >&2
    exit 2
    ;;
esac
"#,
        blob_src = blob_src.display(),
        blob_name = blob_name,
        manifest_src = manifest_src.display(),
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
/// at scope end. Tests serialize on PATH — without serialization they
/// clobber each other in parallel runs.
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
    let blob = b"opaque non-model blob";
    let mut hasher = Sha256::new();
    hasher.update(blob);
    let blob_sha = hex::encode(hasher.finalize());
    let manifest_sha = "1".repeat(64);
    let reference = format!("ghcr.io/example/something@sha256:{manifest_sha}");

    // Non-model artifact-type → annotation not required, sidecar absent.
    let manifest = manifest_json(NON_MODEL_MEDIA_TYPE, None);
    let tools = fake_tool_dir(blob, "thing.bin", 0, &manifest);
    let _path = PathGuard::prepend(&tools);

    let cache = tempfile::tempdir().unwrap();
    let pulled = pull::pull(&reference, &cfg(cache.path().to_path_buf())).unwrap();

    assert_eq!(pulled.sha256_hex, blob_sha);
    assert!(pulled.blob_path.exists(), "blob not in cache");
    let actual_bytes = fs::read(&pulled.blob_path).unwrap();
    assert_eq!(actual_bytes, blob);
    let dir = pulled.blob_path.parent().unwrap();
    assert!(dir.ends_with(&manifest_sha));
    assert!(dir.join("ref.txt").exists());
    let recorded = fs::read_to_string(dir.join("sha256.txt")).unwrap();
    assert_eq!(recorded.trim(), blob_sha);
    // Non-model → no chat-template surface, no sidecar.
    assert_eq!(pulled.chat_template_sha256_hex, None);
    assert!(!dir.join("chat_template.sha256.txt").exists());
}

#[test]
fn pull_refuses_when_cosign_verify_fails() {
    let blob = b"some bytes";
    let manifest_sha = "2".repeat(64);
    let reference = format!("ghcr.io/example/tiny-model@sha256:{manifest_sha}");

    // cosign exits 1 → CosignVerifyFailed. Manifest doesn't matter for
    // this gate (it runs after manifest fetch + annotation read but
    // refusal is the same regardless).
    let manifest = manifest_json(NON_MODEL_MEDIA_TYPE, None);
    let tools = fake_tool_dir(blob, "model.gguf", 1, &manifest);
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

    let manifest = manifest_json(NON_MODEL_MEDIA_TYPE, None);
    let tools = fake_tool_dir(blob, "model.gguf", 0, &manifest);
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

    let manifest = manifest_json(NON_MODEL_MEDIA_TYPE, None);
    let tools = fake_tool_dir(blob, "model.gguf", 0, &manifest);
    let _path = PathGuard::prepend(&tools);

    let cache = tempfile::tempdir().unwrap();
    let pulled = pull::pull(&reference, &cfg(cache.path().to_path_buf())).unwrap();

    // Corrupt the cached blob — next pull MUST refuse, not silently
    // hand back the bad bytes.
    fs::write(&pulled.blob_path, b"tampered").unwrap();

    let err = pull::pull(&reference, &cfg(cache.path().to_path_buf())).unwrap_err();
    assert!(matches!(err, PullError::Sha256Mismatch { .. }), "{err}");
}

#[test]
fn pull_emits_chat_template_sidecar_from_model_artifact_annotation() {
    let blob = b"opaque-gguf-bytes";
    let template_sha = "a".repeat(64);
    let manifest_sha = "5".repeat(64);
    let reference = format!("ghcr.io/example/qwen-tiny@sha256:{manifest_sha}");

    let manifest = manifest_json(MODEL_GGUF_MEDIA_TYPE, Some(&template_sha));
    let tools = fake_tool_dir(blob, "model.gguf", 0, &manifest);
    let _path = PathGuard::prepend(&tools);

    let cache = tempfile::tempdir().unwrap();
    let pulled = pull::pull(&reference, &cfg(cache.path().to_path_buf())).unwrap();

    assert_eq!(
        pulled.chat_template_sha256_hex.as_deref(),
        Some(template_sha.as_str()),
        "annotation value should propagate to the surfaced chat-template digest"
    );
    let dir = pulled.blob_path.parent().unwrap();
    let template_sidecar = dir.join("chat_template.sha256.txt");
    assert!(template_sidecar.exists());
    assert_eq!(
        fs::read_to_string(&template_sidecar).unwrap().trim(),
        template_sha
    );

    // Cache hit reuses the sidecar without re-fetching the manifest.
    fs::write(tools.join("oras"), "#!/bin/sh\nexit 77\n").unwrap();
    chmod_exec(&tools.join("oras"));
    let pulled2 = pull::pull(&reference, &cfg(cache.path().to_path_buf())).unwrap();
    assert_eq!(
        pulled2.chat_template_sha256_hex.as_deref(),
        Some(template_sha.as_str())
    );
}

#[test]
fn pull_refuses_when_model_artifact_lacks_chat_template_annotation() {
    let blob = b"opaque-gguf-bytes";
    let manifest_sha = "6".repeat(64);
    let reference = format!("ghcr.io/example/qwen-tiny@sha256:{manifest_sha}");

    // Model artifact-type but no annotation → publisher misconfiguration,
    // refuse rather than silently leave the F1 binding unmoored.
    let manifest = manifest_json(MODEL_GGUF_MEDIA_TYPE, None);
    let tools = fake_tool_dir(blob, "model.gguf", 0, &manifest);
    let _path = PathGuard::prepend(&tools);

    let cache = tempfile::tempdir().unwrap();
    let err = pull::pull(&reference, &cfg(cache.path().to_path_buf())).unwrap_err();
    assert!(
        matches!(err, PullError::MissingChatTemplateAnnotation { .. }),
        "{err}"
    );
    assert!(
        !cache.path().join(manifest_sha).exists(),
        "missing-annotation artifact must NOT be cached"
    );
}

#[test]
fn pull_refuses_when_chat_template_annotation_is_not_hex() {
    let blob = b"opaque-gguf-bytes";
    let manifest_sha = "7".repeat(64);
    let reference = format!("ghcr.io/example/qwen-tiny@sha256:{manifest_sha}");

    // Annotation present but malformed (uppercase + length 64 → fails
    // the lowercase-hex check). cosign would catch tampering, but the
    // hex validator catches a publisher bug before cosign would run.
    let bad = "DEADBEEF".repeat(8); // 64 chars, but uppercase
    assert_eq!(bad.len(), 64);
    let manifest = manifest_json(MODEL_GGUF_MEDIA_TYPE, Some(&bad));
    let tools = fake_tool_dir(blob, "model.gguf", 0, &manifest);
    let _path = PathGuard::prepend(&tools);

    let cache = tempfile::tempdir().unwrap();
    let err = pull::pull(&reference, &cfg(cache.path().to_path_buf())).unwrap_err();
    assert!(
        matches!(err, PullError::InvalidAnnotationValue { .. }),
        "{err}"
    );
}

#[test]
fn pull_accepts_non_model_artifact_without_annotation() {
    // The devbox image, third-party tooling images, etc. don't declare
    // the model-GGUF media type and aren't required to carry the
    // chat-template annotation — `aegis pull` stays general-purpose.
    let blob = b"some non-model bytes";
    let manifest_sha = "8".repeat(64);
    let reference = format!("ghcr.io/example/devbox@sha256:{manifest_sha}");

    let manifest = manifest_json(NON_MODEL_MEDIA_TYPE, None);
    let tools = fake_tool_dir(blob, "image.tar", 0, &manifest);
    let _path = PathGuard::prepend(&tools);

    let cache = tempfile::tempdir().unwrap();
    let pulled = pull::pull(&reference, &cfg(cache.path().to_path_buf())).unwrap();
    assert_eq!(pulled.chat_template_sha256_hex, None);
    let dir = pulled.blob_path.parent().unwrap();
    assert!(!dir.join("chat_template.sha256.txt").exists());
}

#[test]
fn pull_emits_chat_template_sidecar_for_litertlm_artifact() {
    // Per LiteRT-C (#97): the new application/vnd.aegis-node.model.litertlm.v1
    // artifact-type carries the same chat-template annotation requirement
    // as GGUF, and `aegis pull` writes the same sidecar layout. This test
    // exercises the new media-type acceptance path through the same
    // pull.rs code that handles GGUF — proving the broadening of
    // MODEL_ARTIFACT_TYPES doesn't fork the trust boundary's behavior
    // across formats.
    let blob = b"opaque-litertlm-bytes";
    let template_sha = "b".repeat(64);
    let manifest_sha = "9".repeat(64);
    let reference = format!("ghcr.io/example/gemma-4-e2b-it@sha256:{manifest_sha}");

    let manifest = manifest_json(MODEL_LITERTLM_MEDIA_TYPE, Some(&template_sha));
    let tools = fake_tool_dir(blob, "gemma-4-E2B-it.litertlm", 0, &manifest);
    let _path = PathGuard::prepend(&tools);

    let cache = tempfile::tempdir().unwrap();
    let pulled = pull::pull(&reference, &cfg(cache.path().to_path_buf())).unwrap();

    assert_eq!(
        pulled.chat_template_sha256_hex.as_deref(),
        Some(template_sha.as_str()),
        "annotation should propagate for the litertlm artifact-type identically to gguf"
    );
    let dir = pulled.blob_path.parent().unwrap();
    assert!(dir.join("chat_template.sha256.txt").exists());
}

#[test]
fn pull_refuses_when_litertlm_artifact_lacks_chat_template_annotation() {
    // Same publisher-misconfiguration refusal as the GGUF path —
    // the broadened MODEL_ARTIFACT_TYPES set must enforce the
    // annotation requirement on every member.
    let blob = b"opaque-litertlm-bytes";
    let manifest_sha = "a".repeat(64);
    let reference = format!("ghcr.io/example/gemma-4-e2b-it@sha256:{manifest_sha}");

    let manifest = manifest_json(MODEL_LITERTLM_MEDIA_TYPE, None);
    let tools = fake_tool_dir(blob, "gemma-4-E2B-it.litertlm", 0, &manifest);
    let _path = PathGuard::prepend(&tools);

    let cache = tempfile::tempdir().unwrap();
    let err = pull::pull(&reference, &cfg(cache.path().to_path_buf())).unwrap_err();
    assert!(
        matches!(err, pull::PullError::MissingChatTemplateAnnotation { .. }),
        "{err}"
    );
}
