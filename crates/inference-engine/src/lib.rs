//! Aegis-Node inference engine.
//!
//! Phase 1a deliberately does NOT bind to llama.cpp yet — that lands in
//! v0.9.0 per ADR-014. What this crate ships now is the *runtime
//! mediation harness*: a [`Session`] type that wires [`aegis_identity`],
//! [`aegis_policy`], and [`aegis_ledger_writer`] into a single
//! boot → mediate → shutdown lifecycle.
//!
//! The per-tool-call mediator (rebind → policy → gate → access entry)
//! lands in F0-B (issue #25); this crate gains a `Mediator` type at
//! that point.

pub mod error;
mod session;

pub use error::{Error, Result};
pub use session::{BootConfig, Session};
