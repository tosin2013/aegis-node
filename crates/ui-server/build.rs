//! Build script for `aegis-ui-server`.
//!
//! Sub-phase 1d.1a moved the SPA from a hand-authored `ui/dist/`
//! placeholder to a Vite-built React + Tailwind app. `rust-embed`
//! still bakes `ui/dist/` into the binary at compile time, so we
//! need `ui/dist/` to be present and current. This build script
//! is the bridge between Cargo's compile graph and the SPA's
//! `pnpm install` + `pnpm build`.
//!
//! ## Behaviour
//!
//! 1. Watches the SPA source directory + key config files via
//!    `cargo:rerun-if-changed`. Cargo only invokes this script
//!    when those paths change, so unrelated Rust edits don't pay
//!    the SPA-build cost.
//! 2. If `pnpm` is on `PATH`, runs `pnpm install --frozen-lockfile`
//!    + `pnpm build` in `ui/`. Failure of either is a hard build
//!      failure.
//! 3. If `pnpm` is *not* on `PATH` and `ui/dist/index.html` already
//!    exists, the script skips silently — this is the
//!    "release-tarball ships pre-built dist" path so downstream
//!    `cargo install` users don't need pnpm.
//! 4. If neither `pnpm` nor `ui/dist/` is available, fails with a
//!    clear error directing the operator to install pnpm or
//!    extract a release tarball.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn main() {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo");
    let ui_dir = PathBuf::from(&manifest_dir).join("../../ui");
    let ui_dir = ui_dir
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&manifest_dir).join("../../ui"));

    // Tell cargo when to re-run this script. Cargo skips the entire
    // build script (and so the pnpm invocation) when none of these
    // paths have changed; pure Rust edits don't trigger an SPA
    // rebuild.
    for rel in [
        "src",
        "index.html",
        "package.json",
        "pnpm-lock.yaml",
        "vite.config.ts",
        "tsconfig.json",
        "tsconfig.app.json",
        "tsconfig.node.json",
    ] {
        println!("cargo:rerun-if-changed={}", ui_dir.join(rel).display());
    }

    let dist_index = ui_dir.join("dist/index.html");
    let pnpm_available = pnpm_on_path();

    if !pnpm_available {
        if dist_index.exists() {
            // Release-tarball / pre-built case — leave dist as is.
            println!(
                "cargo:warning=pnpm not found; using existing ui/dist/ at {}",
                dist_index.display()
            );
            return;
        }
        panic!(
            "pnpm not found on PATH and ui/dist/ is empty.\n\
             \n\
             Install pnpm via corepack:\n\
                 corepack enable && corepack prepare pnpm@latest --activate\n\
             \n\
             ...or extract a release tarball that ships pre-built ui/dist/.\n",
        );
    }

    run_pnpm(&ui_dir, &["install", "--frozen-lockfile"]);
    run_pnpm(&ui_dir, &["build"]);
}

/// Returns true iff `pnpm --version` succeeds. Used to gate whether
/// build.rs invokes pnpm or falls through to the pre-built path.
fn pnpm_on_path() -> bool {
    Command::new("pnpm")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run `pnpm <args>` in the SPA directory, propagating stderr/stdout
/// to cargo's output. Hard-fails the build if pnpm exits non-zero.
fn run_pnpm(ui_dir: &Path, args: &[&str]) {
    let status = Command::new("pnpm")
        .args(args)
        .current_dir(ui_dir)
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn pnpm {}: {e}", args.join(" ")));
    if !status.success() {
        panic!(
            "pnpm {} failed with status {} in {}",
            args.join(" "),
            status,
            ui_dir.display(),
        );
    }
}
