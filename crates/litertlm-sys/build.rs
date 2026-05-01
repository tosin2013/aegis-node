//! Build script for `aegis-litertlm-sys`.
//!
//! Three responsibilities:
//!
//! 1. Verify the vendored `c/engine.h` header against the SHA pinned at
//!    the top of this file, then run `bindgen` against it.
//! 2. Locate the prebuilt LiteRT-LM runtime — either via the
//!    `LITERT_LM_PREBUILT_SO` env-var override, or by `oras pull`'ing
//!    the pinned OCI artifact. SHA-verify both `.so` files (the engine
//!    and the constraint-provider sidekick it `DT_NEEDED`s) against
//!    in-tree pins.
//! 3. Emit the dynamic-link directives so downstream crates can call
//!    into the engine `.so`.
//!
//! All three pins (header SHA, engine `.so` SHA, constraint-provider
//! `.so` SHA) match the OCI artifact published by
//! `.github/workflows/litertlm-runtime-publish.yml` and recorded in
//! [ADR-023](../../docs/adrs/023-litertlm-as-second-inference-backend.md)
//! §"Published artifact (current pin)":
//!
//! ```text
//! ghcr.io/tosin2013/aegis-node-runtime/litertlm-linux-amd64
//!   @sha256:e2296f314976f0f9eb1c3e2ef1cc4ab6114ce96ba34f238a3ddbb2d5659e511f
//! ```
//!
//! Resolution policy:
//! - **`LITERT_LM_PREBUILT_SO`** set → use that path, skip network.
//!   Air-gapped contributors and CI staging steps take this path.
//! - **`oras` on PATH** → pull the pinned digest into `OUT_DIR`. CI
//!   uses this path; the canonical recipe.
//! - Neither → build error pointing the operator at the exact
//!   `oras pull` invocation.
//!
//! Either way the SHA-256 of every materialized file is verified
//! before bindgen runs — a tampered or wrong-version artifact fails
//! the build with a precise actionable error rather than silently
//! linking against the wrong runtime.

use std::env;
use std::path::PathBuf;
use std::process::Command;

use sha2::{Digest, Sha256};

/// SHA-256 of `c/engine.h` at LiteRT-LM tag `v0.10.2` (commit
/// `476c0bd49429569b2a4685c4db7a657d531d4b6e`). This is the same SHA
/// recorded in the LiteRT-0 OCI artifact's `dev.aegis-node.runtime.header.sha256`
/// annotation. Bumping the upstream pin is an explicit, reviewed change
/// to this constant.
const EXPECTED_HEADER_SHA: &str =
    "cacee1d18aa9e2c22aeb8da2fc1576b25c03d7104e5319a0352c64a57bb691e9";

/// SHA-256 of `libaegis_litertlm_engine_cpu.so` produced by the LiteRT-0
/// publish run for tag v0.10.2. Recorded in the OCI artifact's
/// `dev.aegis-node.runtime.so.sha256` annotation. Bumping the upstream
/// pin is an explicit, reviewed change to this constant.
const EXPECTED_SO_SHA: &str = "216451eb3726b3326dbadbdc08ec2eda44d45d3035167d8613f45e08eb80a012";

/// SHA-256 of `libGemmaModelConstraintProvider.so` — the
/// constrained-decoding sidekick `engine_cpu_shared` has a `DT_NEEDED`
/// entry for. Vendored, LFS-tracked in the upstream repo at
/// `prebuilt/linux_x86_64/libGemmaModelConstraintProvider.so` (size
/// ~22.76 MB). The publish workflow ships both files in the same OCI
/// artifact so the dynamic linker can find the constraint provider via
/// the engine's rpath.
///
/// SHA captured from the upstream LFS pointer at tag v0.10.2:
/// `oid sha256:b30101a057a69d2c877266ac7373023864816ccaed7d9413d97b98ae12842009`.
/// Recorded in the OCI artifact's
/// `dev.aegis-node.runtime.constraint_provider_so.sha256` annotation.
const EXPECTED_GEMMA_CONSTRAINT_PROVIDER_SO_SHA: &str =
    "b30101a057a69d2c877266ac7373023864816ccaed7d9413d97b98ae12842009";

/// Pinned OCI reference of the LiteRT-LM runtime artifact this build
/// script expects. Recorded so error messages can point operators at
/// the exact `oras pull` / `aegis pull` invocation that materializes
/// the `.so` files. The artifact carries two layers — the engine
/// `.so` and `libGemmaModelConstraintProvider.so` — both verified
/// against the SHA constants above.
const PINNED_OCI_REF: &str = "ghcr.io/tosin2013/aegis-node-runtime/litertlm-linux-amd64\
     @sha256:e2296f314976f0f9eb1c3e2ef1cc4ab6114ce96ba34f238a3ddbb2d5659e511f";

/// Filename of the engine `.so` (used for the
/// `cargo:rustc-link-lib=dylib=...` directive — name strips the `lib`
/// prefix and `.so` suffix).
const SO_FILENAME: &str = "libaegis_litertlm_engine_cpu.so";

/// Filename of the constraint-provider `.so`. Not link-cited (only
/// loaded transitively via the engine's `DT_NEEDED`), but verified
/// and placed alongside the engine `.so` in the staging dir so the
/// dynamic linker finds it via rpath.
const GEMMA_SO_FILENAME: &str = "libGemmaModelConstraintProvider.so";

/// Library name passed to `cargo:rustc-link-lib=dylib=`.
const LINK_LIB_NAME: &str = "aegis_litertlm_engine_cpu";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=c/engine.h");
    println!("cargo:rerun-if-env-changed=LITERT_LM_PREBUILT_SO");
    println!("cargo:rerun-if-env-changed=DOCS_RS");

    if let Err(e) = run() {
        // Use cargo's structured error output so the message lands in
        // the build log under a clear marker. We exit with code 1 via
        // process::exit rather than panic, so cargo's own panic-handling
        // doesn't add noise to what is already a precise error.
        println!("cargo:warning=aegis-litertlm-sys build failed: {e}");
        eprintln!("\naegis-litertlm-sys: {e}\n");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    // docs.rs builds with no network and no prebuilt — emit empty
    // bindings so docs build succeeds. Downstream crates compile
    // against the same surface; only the link step would fail, and
    // docs.rs doesn't link.
    if env::var_os("DOCS_RS").is_some() {
        return write_docs_rs_stub();
    }

    let manifest_dir = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR")
            .map_err(|_| "CARGO_MANIFEST_DIR unset (cargo bug?)".to_string())?,
    );
    let out_dir =
        PathBuf::from(env::var("OUT_DIR").map_err(|_| "OUT_DIR unset (cargo bug?)".to_string())?);

    let header_path = manifest_dir.join("c").join("engine.h");
    verify_sha256(&header_path, EXPECTED_HEADER_SHA, "vendored c/engine.h")?;

    let so_path = locate_and_verify_so()?;
    let so_dir = so_path
        .parent()
        .ok_or_else(|| format!("LITERT_LM_PREBUILT_SO has no parent: {}", so_path.display()))?
        .to_path_buf();

    // The engine .so has a DT_NEEDED entry for libGemmaModelConstraintProvider.so —
    // verify that file is present in the same staging dir and matches
    // its SHA-256 pin. The dynamic linker resolves it through the
    // engine .so's rpath at session-load time, so the file must sit
    // beside the engine .so we just verified.
    let gemma_so_path = so_dir.join(GEMMA_SO_FILENAME);
    verify_gemma_constraint_provider(&gemma_so_path)?;

    // Emit cargo directives so downstream crates pick up the dylib at
    // link time and at runtime (rpath). Adding the directory to both
    // the link search path AND the rpath lets `cargo test` and `cargo
    // run` find the .so without requiring LD_LIBRARY_PATH gymnastics.
    println!("cargo:rustc-link-search=native={}", so_dir.display());
    println!("cargo:rustc-link-lib=dylib={LINK_LIB_NAME}");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", so_dir.display());

    let bindings = bindgen::Builder::default()
        .header(
            header_path
                .to_str()
                .ok_or_else(|| format!("non-UTF8 header path: {}", header_path.display()))?,
        )
        .clang_arg("-x")
        .clang_arg("c")
        .allowlist_function("litert_lm_.*")
        .allowlist_type("LiteRtLm.*")
        .allowlist_type("InputData.*")
        .allowlist_type("InputDataType")
        .allowlist_type("Type")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .layout_tests(false)
        .generate()
        .map_err(|e| format!("bindgen failed against c/engine.h: {e}"))?;

    let bindings_path = out_dir.join("bindings.rs");
    bindings.write_to_file(&bindings_path).map_err(|e| {
        format!(
            "failed to write bindings to {}: {e}",
            bindings_path.display()
        )
    })?;

    Ok(())
}

/// Locate the prebuilt LiteRT-LM runtime `.so`.
///
/// Resolution order:
/// 1. `LITERT_LM_PREBUILT_SO` env var: must be a path to the `.so`
///    file (not a directory). SHA-256 verified against
///    [`EXPECTED_SO_SHA`].
/// 2. Try `oras` on PATH against [`PINNED_OCI_REF`]; pull the
///    blob into `OUT_DIR` and return that path. Falls through if
///    `oras` is not installed or the pull fails.
///
/// Otherwise returns a structured error pointing the operator at the
/// exact recipe to materialize the file.
fn locate_and_verify_so() -> Result<PathBuf, String> {
    if let Some(p) = env::var_os("LITERT_LM_PREBUILT_SO") {
        let path = PathBuf::from(p);
        if !path.is_file() {
            return Err(format!(
                "LITERT_LM_PREBUILT_SO is set but does not point at a regular file: {}\n\
                 Expected: a path to {SO_FILENAME} matching SHA-256 {EXPECTED_SO_SHA}.",
                path.display()
            ));
        }
        verify_sha256(&path, EXPECTED_SO_SHA, "LiteRT-LM runtime .so")?;
        return Ok(path);
    }

    // Best-effort `oras pull` against the pinned digest. We don't make
    // network access mandatory — air-gapped contributors set the env
    // var instead.
    if let Some(out_dir) = env::var_os("OUT_DIR") {
        let staging = PathBuf::from(out_dir).join("litertlm-runtime");
        if let Some(found) = try_oras_pull(&staging) {
            verify_sha256(&found, EXPECTED_SO_SHA, "LiteRT-LM runtime .so (oras pull)")?;
            return Ok(found);
        }
    }

    Err(format!(
        "LITERT_LM_PREBUILT_SO is unset and `oras pull` was unavailable or failed.\n\
         \n\
         Materialize the runtime artifact and re-run cargo build:\n\
         \n\
             oras pull {PINNED_OCI_REF} -o /tmp/litertlm-runtime\n\
             export LITERT_LM_PREBUILT_SO=/tmp/litertlm-runtime/{SO_FILENAME}\n\
             cargo build -p aegis-litertlm-sys\n\
         \n\
         (The .so is published by .github/workflows/litertlm-runtime-publish.yml\n\
          and pinned in docs/adrs/023-litertlm-as-second-inference-backend.md.\n\
          Expected SHA-256 of the .so: {EXPECTED_SO_SHA})"
    ))
}

/// Try to pull the pinned OCI artifact via `oras`. Returns the path to
/// the materialized `.so` on success, or `None` if `oras` is not
/// available or the pull failed for any reason. The caller decides
/// whether the failure is fatal.
fn try_oras_pull(staging: &PathBuf) -> Option<PathBuf> {
    if let Err(e) = std::fs::create_dir_all(staging) {
        println!(
            "cargo:warning=could not create oras staging dir {}: {e}",
            staging.display()
        );
        return None;
    }

    let status = Command::new("oras")
        .args(["pull", PINNED_OCI_REF, "-o"])
        .arg(staging)
        .status();

    match status {
        Ok(s) if s.success() => {
            let candidate = staging.join(SO_FILENAME);
            if candidate.is_file() {
                Some(candidate)
            } else {
                println!(
                    "cargo:warning=oras pull succeeded but {} was not written; staging contents may differ",
                    candidate.display()
                );
                None
            }
        }
        Ok(s) => {
            println!("cargo:warning=oras pull exited {s}");
            None
        }
        Err(e) => {
            // `oras` not on PATH is the common case — it's not an
            // error in itself, just means the env-var path is required.
            println!("cargo:warning=oras not available ({e}); set LITERT_LM_PREBUILT_SO instead");
            None
        }
    }
}

/// Locate and verify the constraint-provider sidekick that
/// `libaegis_litertlm_engine_cpu.so` `DT_NEEDED`s.
///
/// The publish workflow bundles it as a second blob in the OCI
/// artifact; `oras pull` materializes both files into the same
/// staging directory. If the file is missing (e.g. the operator
/// pointed `LITERT_LM_PREBUILT_SO` at a hand-built engine .so without
/// staging the constraint provider next to it), surface a precise
/// error message that names the missing file and the SHA they need.
fn verify_gemma_constraint_provider(path: &PathBuf) -> Result<(), String> {
    if !path.is_file() {
        return Err(format!(
            "{GEMMA_SO_FILENAME} not found at {}\n\
             \n\
             The engine .so has a DT_NEEDED entry for this file (LiteRT-LM's\n\
             grammar-constrained sampler — used by Gemma's tool-call decoder).\n\
             It must sit in the same directory as {SO_FILENAME} so the dynamic\n\
             linker resolves it via rpath at session-load time.\n\
             \n\
             Materialize it via `oras pull` against the rebundled artifact:\n\
             \n\
                 oras pull {PINNED_OCI_REF} -o /tmp/litertlm-runtime\n\
             \n\
             (Expected SHA-256: {EXPECTED_GEMMA_CONSTRAINT_PROVIDER_SO_SHA})",
            path.display()
        ));
    }
    verify_sha256(
        path,
        EXPECTED_GEMMA_CONSTRAINT_PROVIDER_SO_SHA,
        "libGemmaModelConstraintProvider.so",
    )
}

fn verify_sha256(path: &PathBuf, expected_hex: &str, label: &str) -> Result<(), String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("could not read {label} at {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let actual = hex::encode(hasher.finalize());
    if actual != expected_hex {
        return Err(format!(
            "{label} SHA-256 mismatch:\n  path:     {}\n  expected: {expected_hex}\n  actual:   {actual}\n\
             This indicates the file was modified, replaced, or built from a different upstream tag.\n\
             Refusing to proceed.",
            path.display()
        ));
    }
    Ok(())
}

/// Emit a stub `bindings.rs` for docs.rs builds (no network, no
/// prebuilt artifact). Docs.rs builds the `aegis-litertlm-sys` crate
/// only to extract its public surface; it never executes any FFI
/// call, so a syntactically valid empty stub is enough.
fn write_docs_rs_stub() -> Result<(), String> {
    let out_dir =
        PathBuf::from(env::var("OUT_DIR").map_err(|_| "OUT_DIR unset (cargo bug?)".to_string())?);
    let bindings_path = out_dir.join("bindings.rs");
    std::fs::write(
        &bindings_path,
        "// docs.rs stub — no FFI surface emitted.\n",
    )
    .map_err(|e| format!("failed to write docs.rs stub: {e}"))?;
    Ok(())
}
