//! Aegis-Node workload identity.
//!
//! SPIFFE-compatible workload identity per ADR-003 (F1). This crate owns:
//!
//! - SPIFFE ID parsing in the strict Aegis form
//!   `spiffe://<trust-domain>/agent/<workload>/<instance>` ([`SpiffeId`]).
//! - A file-backed local CA ([`LocalCa`]) for one-time-init / repeat-issue,
//!   used by the `aegis` CLI on developer laptops.
//! - X.509-SVID issuance ([`X509Svid`]) with the SPIFFE URI in the SAN and
//!   a `(model, manifest, config)` SHA-256 digest triple bound into a custom
//!   extension. Any digest change invalidates the identity (per ADR-003).
//!
//! Phase 0 ships the local-CA flavor only. Phase 2 swaps `LocalCa` for SPIRE
//! workload-attestation; the SVID format on the wire stays identical.

mod binding;
mod ca;
pub mod error;
pub mod ffi;
mod spiffe;
mod svid;

pub use binding::{
    verify_chat_template_binding, verify_digest_binding, DigestField, DigestMismatch,
};
pub use ca::{
    extract_chat_template_from_pem, extract_digest_triple_from_pem, extract_spiffe_id_from_pem,
    LocalCa,
};
pub use error::{Error, Result};
pub use spiffe::SpiffeId;
pub use svid::{
    Digest, DigestTriple, X509Svid, CHAT_TEMPLATE_BINDING_LEN, CHAT_TEMPLATE_BINDING_OID,
    DIGEST_BINDING_LEN, DIGEST_BINDING_OID,
};
