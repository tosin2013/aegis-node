//! F1 digest-rebind glue: verify the SVID's bound triple against live
//! digests, and on mismatch emit an `EntryType::Violation` to the ledger.
//!
//! The runtime is expected to call this on session start and at every
//! `CheckPermission` boundary. A returned `Error::IdentityRebind` MUST
//! halt the agent — the ledger entry has already been written by the
//! time this function returns the error.
//!
//! Why not split this off into a separate "halt" helper? Because the
//! "emit then halt" sequence is the entire point of the F1 invariant:
//! the audit record must exist before the process dies. Keeping them in
//! one function makes "did we forget to log?" impossible.

use aegis_identity::{verify_digest_binding, DigestTriple};
use aegis_ledger_writer::LedgerWriter;
use chrono::{DateTime, Utc};

use crate::error::{Error, Result};
use crate::violation::{emit_violation, ViolationEvent};

/// Verify the SVID's `(model, manifest, config)` binding against the live
/// triple. Returns `Ok(())` on match. On mismatch, writes a Violation
/// entry to `writer` and returns `Error::IdentityRebind`. On SVID parse
/// failure, returns `Error::Identity` without writing anything (a
/// malformed cert is a different class of problem).
pub fn check_identity_binding(
    writer: &mut LedgerWriter,
    agent_identity_hash: [u8; 32],
    cert_pem: &str,
    live: &DigestTriple,
    timestamp: DateTime<Utc>,
) -> Result<()> {
    match verify_digest_binding(cert_pem, live)? {
        None => Ok(()),
        Some(mismatch) => {
            let event = ViolationEvent {
                reason: format!("identity digest binding violated: {mismatch}"),
                resource_uri: Some(format!("digest-binding://{}", mismatch.field)),
                access_type: None,
                timestamp,
            };
            emit_violation(writer, agent_identity_hash, event)?;
            Err(Error::IdentityRebind(mismatch))
        }
    }
}

/// Convenience wrapper that supplies `Utc::now()` for the violation
/// timestamp. The test variant takes an explicit timestamp because
/// fixture-driven runs want deterministic clocks.
pub fn check_identity_binding_now(
    writer: &mut LedgerWriter,
    agent_identity_hash: [u8; 32],
    cert_pem: &str,
    live: &DigestTriple,
) -> Result<()> {
    check_identity_binding(writer, agent_identity_hash, cert_pem, live, Utc::now())
}
