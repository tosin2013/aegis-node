//! Compile-time embed of the SPA's static-asset output.
//!
//! `ui/dist/` is the canonical location for the built SPA per
//! [ADR-031](../../../docs/adrs/031-community-webui-for-local-collaboration.md).
//! Sub-phase 1d.0 ships hand-authored placeholder HTML/CSS in that
//! directory; sub-phase 1d.1 replaces them with the Vite build
//! output. The embed path doesn't change between sub-phases — the
//! SPA build target is `ui/dist/` and `rust-embed` bakes whatever's
//! there into the binary at `cargo build` time.

use rust_embed::Embed;

/// Compile-time handle on the `ui/dist/` directory.
///
/// `rust-embed` resolves the path relative to the crate's
/// `Cargo.toml`. The `debug-embed` feature is set so that even in
/// debug builds we read from the baked-in copy, not from disk —
/// otherwise the placeholder served in tests would shift if the
/// repo's `ui/dist/` were edited mid-test.
#[derive(Embed)]
#[folder = "$CARGO_MANIFEST_DIR/../../ui/dist"]
pub struct UiDist;
