//! HTTP handler modules. Each submodule owns a small, related set of
//! routes — the `lib::router` function wires them together.

pub mod assets;
pub mod health;
pub mod manifests;
pub mod models;
pub mod validate;
