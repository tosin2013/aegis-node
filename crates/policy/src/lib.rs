//! Aegis-Node manifest enforcement engine (F2 per ADR-004 + ADR-008 + ADR-009).
//!
//! Compiles a parsed Permission Manifest into a [`Policy`] that answers
//! `Allow / Deny / RequireApproval` for each I/O attempt the runtime
//! mediates. Closed-by-default: the manifest must affirmatively grant an
//! operation; silence is denial.
//!
//! Phase 1a accepts single-file manifests (no `extends:` resolution). The
//! Go validator (`pkg/manifest`) owns extends and narrowing checks; the
//! resolved manifest is what should reach this engine. Once the control
//! plane lands, `Policy::from_resolved_json` will be the canonical entry
//! point and the YAML loader becomes a CLI/dev convenience.

pub mod decision;
pub mod error;
mod identity_binding;
pub mod manifest;
mod policy;
mod violation;

pub use decision::{Decision, NetworkProto};
pub use error::{Error, Result};
pub use identity_binding::{check_identity_binding, check_identity_binding_now};
pub use manifest::{
    Agent, ApiGrant, ApprovalClass, Filesystem, Identity, Manifest, Network, NetworkAllowEntry,
    NetworkMode, NetworkPolicy, NetworkProtocol, Tools, WriteAction, WriteGrant,
};
pub use policy::Policy;
pub use violation::{emit_violation, ViolationEvent};
