//! Digest-rebind verification for issued SVIDs (F1 per ADR-003).
//!
//! At every `CheckPermission` (and at session start), the runtime must
//! verify that the live `(model, manifest, config)` digest triple still
//! matches the one bound into the SVID at issuance time. Any drift means
//! the agent's identity no longer attests to the artifacts it's running
//! — the runtime halts.
//!
//! This module is the pure-comparison half of that check. The
//! ledger-emit-and-halt half lives in `aegis-policy::check_identity_binding`
//! so this crate stays free of write-side dependencies.

use std::fmt;

use crate::ca::{extract_chat_template_from_pem, extract_digest_triple_from_pem};
use crate::error::Result;
use crate::svid::{Digest, DigestTriple};

/// Which of the bound artifacts changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestField {
    Model,
    Manifest,
    Config,
    /// GGUF `tokenizer.chat_template` SHA-256 (per ADR-022 / OCI-B).
    /// Bound via a separate optional X.509 extension
    /// (`CHAT_TEMPLATE_BINDING_OID`); only relevant when the SVID was
    /// issued with a chat-template digest.
    ChatTemplate,
}

impl DigestField {
    pub fn name(self) -> &'static str {
        match self {
            DigestField::Model => "model",
            DigestField::Manifest => "manifest",
            DigestField::Config => "config",
            DigestField::ChatTemplate => "chat_template",
        }
    }
}

impl fmt::Display for DigestField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// One mismatched field, with both the bound and the live digest so an
/// auditor can trace what changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestMismatch {
    pub field: DigestField,
    pub bound: Digest,
    pub live: Digest,
}

impl fmt::Display for DigestMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} digest changed (bound={}, live={})",
            self.field,
            self.bound.hex(),
            self.live.hex()
        )
    }
}

/// Verify the SVID's bound digest triple against `live`. Returns:
///
/// - `Ok(None)` — full match, identity binding intact.
/// - `Ok(Some(mismatch))` — the first field that drifted (order:
///   model → manifest → config; deterministic so audit can replay).
/// - `Err(_)` — the SVID itself was unparseable (cert format problem,
///   not a binding violation).
///
/// Cheap: a string→DER parse plus three 32-byte compares. Fine to call
/// per `CheckPermission`.
pub fn verify_digest_binding(
    cert_pem: &str,
    live: &DigestTriple,
) -> Result<Option<DigestMismatch>> {
    let bound = extract_digest_triple_from_pem(cert_pem)?;
    if bound.model != live.model {
        return Ok(Some(DigestMismatch {
            field: DigestField::Model,
            bound: bound.model,
            live: live.model,
        }));
    }
    if bound.manifest != live.manifest {
        return Ok(Some(DigestMismatch {
            field: DigestField::Manifest,
            bound: bound.manifest,
            live: live.manifest,
        }));
    }
    if bound.config != live.config {
        return Ok(Some(DigestMismatch {
            field: DigestField::Config,
            bound: bound.config,
            live: live.config,
        }));
    }
    Ok(None)
}

/// Verify the SVID's chat-template binding (per ADR-022 / OCI-B).
///
/// Semantics, ordered by the way they show up at runtime:
///
/// - SVID has no chat-template extension AND `live` is `None` →
///   `Ok(None)`. Nothing was bound, nothing was claimed; not a violation.
/// - SVID has no chat-template extension AND `live` is `Some(_)` →
///   `Ok(None)`. The runtime computed a chat-template digest but the
///   SVID didn't promise anything about it; treat as informational, not
///   a violation. (If you want to *require* a binding, refuse at boot
///   when the operator passes a `live` digest but the issuer wasn't
///   given one — that's a configuration issue, not a rebind.)
/// - SVID has a chat-template extension AND `live` is `Some(d)` AND
///   `bound == d` → `Ok(None)`. Match.
/// - SVID has a chat-template extension AND `live` is `Some(d)` AND
///   `bound != d` → `Ok(Some(mismatch))`. Drift, halt at the call site.
/// - SVID has a chat-template extension AND `live` is `None` →
///   `Ok(Some(mismatch))` with `live = Digest::ZERO`. The SVID claims a
///   template but nothing was provided to compare; that's unsafe
///   ambiguity and we refuse.
/// - Cert-format problems → `Err(_)`.
pub fn verify_chat_template_binding(
    cert_pem: &str,
    live: Option<&Digest>,
) -> Result<Option<DigestMismatch>> {
    let bound = extract_chat_template_from_pem(cert_pem)?;
    match (bound, live) {
        (None, _) => Ok(None),
        (Some(b), Some(l)) if &b == l => Ok(None),
        (Some(b), Some(l)) => Ok(Some(DigestMismatch {
            field: DigestField::ChatTemplate,
            bound: b,
            live: *l,
        })),
        (Some(b), None) => Ok(Some(DigestMismatch {
            field: DigestField::ChatTemplate,
            bound: b,
            live: Digest([0u8; 32]),
        })),
    }
}
