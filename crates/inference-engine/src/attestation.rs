//! F6 end-of-session signed network attestation (issue #37).
//!
//! Every session — even one with zero outbound connections — must
//! produce an `EntryType::NetworkAttestation` summarizing the network
//! activity it observed. "No attestation entry" is not equivalent to
//! "zero connections"; an auditor seeing the former MUST treat the
//! ledger as malformed.
//!
//! The summary carries:
//! - `totalConnections` / `allowedCount` / `approvedCount` / `deniedCount`
//! - `connectionsDigestHex`: SHA-256 of the canonical-JSON connection
//!   list (sorted-keys form), so a verifier can independently recompute
//!   the same digest from the F4 access entries that preceded.
//! - `signatureHex`: HMAC-SHA-256 over the summary canonical bytes,
//!   keyed on `SHA-256(svid_private_key_pem)`.
//!
//! ## Phase 1a signing trade-off
//!
//! HMAC-SHA-256 means the verifier needs the SVID private key to check
//! the signature. That's adequate for self-attestation tamper-evidence
//! ON TOP of the F9 chain (which already prevents post-hoc entry edits),
//! but not for offline replay verification from the public certificate
//! alone. Asymmetric signing (ECDSA-P256 over the same payload) lands
//! when the identity FFI surface (#17 / cdylib) exposes a `sign()`
//! method — filed as a v0.9.0 follow-up.

use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::{Digest as _, Sha256};

use aegis_ledger_writer::{Entry, EntryType};
use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::session::{NetworkConnectionDecision, NetworkConnectionMeta, Session};

type HmacSha256 = Hmac<Sha256>;

/// Compute the canonical (sorted-keys, no whitespace) JSON representation
/// of a single connection entry. Public so verifiers can recompute it.
pub fn canonical_connection_json(meta: &NetworkConnectionMeta) -> Value {
    let mut obj = Map::new();
    obj.insert("host".to_string(), Value::String(meta.host.clone()));
    obj.insert(
        "port".to_string(),
        Value::Number(serde_json::Number::from(meta.port)),
    );
    obj.insert("protocol".to_string(), Value::String(meta.protocol.clone()));
    obj.insert(
        "decision".to_string(),
        Value::String(decision_str(meta.decision).to_string()),
    );
    obj.insert(
        "timestamp".to_string(),
        Value::String(
            meta.timestamp
                .to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
        ),
    );
    Value::Object(obj)
}

fn decision_str(d: NetworkConnectionDecision) -> &'static str {
    match d {
        NetworkConnectionDecision::Allowed => "allowed",
        NetworkConnectionDecision::Approved => "approved",
        NetworkConnectionDecision::Denied => "denied",
    }
}

/// Derive the HMAC key from the SVID private key PEM by SHA-256ing it.
/// Verifier runs the same derivation; mismatch on either side breaks
/// the signature.
pub fn derive_attestation_key(svid_private_key_pem: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(svid_private_key_pem.as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&hasher.finalize());
    out
}

/// Compute the HMAC-SHA-256 signature over the canonical-JSON
/// representation of a summary object containing exactly the fields
/// listed in `SIGNED_FIELDS`. Returns `None` only if HMAC
/// initialization fails, which is unreachable for a fixed 32-byte
/// key but clippy::expect_used demands the explicit fallback.
pub fn compute_signature(key: &[u8; 32], summary_without_signature: &Value) -> Option<[u8; 32]> {
    let canonical = serde_json::to_vec(summary_without_signature).ok()?;
    let mut mac = HmacSha256::new_from_slice(key).ok()?;
    mac.update(&canonical);
    let mut out = [0u8; 32];
    out.copy_from_slice(&mac.finalize().into_bytes());
    Some(out)
}

/// The exact set of fields the signer HMACs over. Listed explicitly so
/// that `verify_signature` can recompute the canonical view from a full
/// ledger entry (which carries entry-level fields like `entryId` /
/// `prevHash` and the unsigned `networkConnectionsObserved` array
/// alongside the signed summary).
const SIGNED_FIELDS: &[&str] = &[
    "totalConnections",
    "allowedCount",
    "approvedCount",
    "deniedCount",
    "connectionsDigestHex",
];

/// Verify an attestation entry's signature. Accepts either the bare
/// summary payload or the full ledger entry — the function selects
/// only the fields in `SIGNED_FIELDS` before recomputing the HMAC, so
/// callers don't have to strip writer-injected metadata themselves.
pub fn verify_signature(svid_private_key_pem: &str, entry: &Value) -> bool {
    let Some(obj) = entry.as_object() else {
        return false;
    };
    let Some(sig_hex) = obj.get("signatureHex").and_then(|v| v.as_str()) else {
        return false;
    };
    let Ok(expected) = hex::decode(sig_hex) else {
        return false;
    };
    let mut signed_view = Map::new();
    for field in SIGNED_FIELDS {
        let Some(v) = obj.get(*field) else {
            return false;
        };
        signed_view.insert((*field).to_string(), v.clone());
    }
    let key = derive_attestation_key(svid_private_key_pem);
    let Some(actual) = compute_signature(&key, &Value::Object(signed_view)) else {
        return false;
    };
    expected.as_slice() == actual.as_slice()
}

/// Emit one `EntryType::NetworkAttestation` ledger entry summarizing
/// the session's network activity. Called by `Session::shutdown`
/// before `SessionEnd`. A zero-connection session still emits one
/// entry with all counts == 0.
pub(crate) fn emit_network_attestation(session: &mut Session) -> Result<()> {
    let log = session.network_log.clone();
    let total = log.len() as u64;
    let allowed = log
        .iter()
        .filter(|m| m.decision == NetworkConnectionDecision::Allowed)
        .count() as u64;
    let approved = log
        .iter()
        .filter(|m| m.decision == NetworkConnectionDecision::Approved)
        .count() as u64;
    let denied = log
        .iter()
        .filter(|m| m.decision == NetworkConnectionDecision::Denied)
        .count() as u64;

    let connections_array: Value =
        Value::Array(log.iter().map(canonical_connection_json).collect());
    let connections_canonical = serde_json::to_vec(&connections_array).map_err(Error::Serde)?;
    let mut digest_hasher = Sha256::new();
    digest_hasher.update(&connections_canonical);
    let connections_digest_hex = hex::encode(digest_hasher.finalize());

    // Build the summary WITHOUT the signature field, then hash, then
    // append the signature.
    let mut summary = Map::new();
    summary.insert(
        "totalConnections".to_string(),
        Value::Number(serde_json::Number::from(total)),
    );
    summary.insert(
        "allowedCount".to_string(),
        Value::Number(serde_json::Number::from(allowed)),
    );
    summary.insert(
        "approvedCount".to_string(),
        Value::Number(serde_json::Number::from(approved)),
    );
    summary.insert(
        "deniedCount".to_string(),
        Value::Number(serde_json::Number::from(denied)),
    );
    summary.insert(
        "connectionsDigestHex".to_string(),
        Value::String(connections_digest_hex),
    );

    let key = derive_attestation_key(session.key_pem());
    let sig = compute_signature(&key, &Value::Object(summary.clone())).ok_or_else(|| {
        Error::SvidSelfCheck {
            field: "attestation_hmac".to_string(),
        }
    })?;
    summary.insert("signatureHex".to_string(), Value::String(hex::encode(sig)));

    // Network connections observed list — kept on a separate field so
    // verifiers can independently recompute connectionsDigestHex from
    // the access entries that preceded.
    summary.insert("networkConnectionsObserved".to_string(), connections_array);

    let agent_hash = session.agent_identity_hash();
    let session_id = session.session_id().to_string();
    session.ledger_writer_mut().append(Entry {
        session_id,
        entry_type: EntryType::NetworkAttestation,
        agent_identity_hash: agent_hash,
        timestamp: Utc::now(),
        payload: summary,
    })?;
    Ok(())
}
