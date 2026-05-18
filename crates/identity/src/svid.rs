//! X.509-SVID representation for issued workload identities.
//!
//! Per ADR-003 (F1) the SVID binds a SPIFFE ID to a SHA-256 digest triple
//! `(model, manifest, config)`. The digest triple is carried in a single
//! private custom X.509 extension; verifiers re-extract and compare it
//! against live digests at every `CheckPermission`.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::spiffe::SpiffeId;

/// Aegis private digest-binding OID. **Placeholder** — slated for replacement
/// once we register an OID under our enterprise arc; the Compatibility Charter
/// freezes the *format* of this extension, not the OID, so changing it is a
/// non-breaking on-disk change for unverified deployments.
pub const DIGEST_BINDING_OID: &[u64] = &[1, 3, 6, 1, 4, 1, 99999, 1];

/// Length of the digest-binding extension payload (model || manifest || config).
pub const DIGEST_BINDING_LEN: usize = 32 * 3;

/// Aegis private chat-template-binding OID — separate extension from the
/// `(model, manifest, config)` triple so adding it doesn't break wire-format
/// compatibility with SVIDs issued before OCI-B (per [ADR-022](../../docs/adrs/022-trust-boundary-format-agnosticism.md)).
/// The extension is **optional**: SVIDs issued without an upstream chat-template
/// claim simply omit it. Verifiers treat absence as "no chat-template binding,"
/// not as a violation.
///
/// Payload is a single 32-byte SHA-256 of the GGUF's `tokenizer.chat_template`
/// bytes — the same value the publisher set in the cosign-covered manifest
/// annotation `dev.aegis-node.chat-template.sha256` (see
/// `aegis_cli::pull::CHAT_TEMPLATE_SHA_ANNOTATION`).
pub const CHAT_TEMPLATE_BINDING_OID: &[u64] = &[1, 3, 6, 1, 4, 1, 99999, 2];

/// Length of the chat-template-binding extension payload.
pub const CHAT_TEMPLATE_BINDING_LEN: usize = 32;

/// Aegis private turn-binding OID (ADR-030, F1 extension). Carried as a
/// non-critical custom extension on per-turn SVIDs issued by
/// [`crate::LocalCa::issue_turn_svid`]. Records the turn's audience
/// claim so a stolen SVID from turn N cannot be replayed at turn M:
/// verifiers reject a cert whose embedded audience disagrees with the
/// turn the request is bound to.
///
/// The extension is **only emitted on per-turn SVIDs**. Session-long
/// SVIDs ([`crate::LocalCa::issue_svid`] +
/// [`crate::LocalCa::issue_svid_with_chat_template`]) omit it. A verifier
/// that sees a TURN_BINDING extension knows the SVID is turn-scoped;
/// a verifier that sees only DIGEST_BINDING (+ optional CHAT_TEMPLATE)
/// knows the SVID is session-scoped.
pub const TURN_BINDING_OID: &[u64] = &[1, 3, 6, 1, 4, 1, 99999, 3];

/// Turn binding wire format inside the [`TURN_BINDING_OID`] extension.
///
/// Layout: `[2 bytes big-endian audience length N][N bytes UTF-8 audience]`.
///
/// The audience is a stable URI string of the form
/// `aegis-turn://<session_id>/<turn_number>` (per ADR-030 §"Identity
/// claim shape"). Length prefix lets future formats append more fields
/// (turn context-digest, attestation selectors) without breaking
/// existing parsers — they decode the prefix-bounded audience and stop.
///
/// Maximum encoded length is bounded by [`MAX_TURN_BINDING_LEN`] so
/// the extension stays a fixed size budget; a session ID + small turn
/// number always fits well inside it.
pub const MAX_TURN_BINDING_LEN: usize = 2 + 512;

/// Decoded turn-binding extension. `audience` is the freshly-minted
/// URI for the per-turn SVID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnBinding {
    pub audience: String,
}

impl TurnBinding {
    /// Encode the turn binding into the wire format for the X.509
    /// extension. Errors if the audience is too long to fit the
    /// 2-byte length prefix or is empty.
    pub fn encode(&self) -> Result<Vec<u8>> {
        let bytes = self.audience.as_bytes();
        if bytes.is_empty() {
            return Err(Error::CertParse(
                "turn binding audience must not be empty".to_string(),
            ));
        }
        if bytes.len() > u16::MAX as usize {
            return Err(Error::CertParse(format!(
                "turn binding audience too long: {} bytes",
                bytes.len()
            )));
        }
        let len = u16::try_from(bytes.len()).map_err(|_| {
            Error::CertParse("turn binding audience exceeds u16 length".to_string())
        })?;
        let mut out = Vec::with_capacity(2 + bytes.len());
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(bytes);
        Ok(out)
    }

    /// Decode the wire format. Errors on short input, mismatched
    /// length prefix, or non-UTF-8 audience bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 2 {
            return Err(Error::CertParse(format!(
                "turn binding truncated: {} bytes",
                bytes.len()
            )));
        }
        let len = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
        if bytes.len() < 2 + len {
            return Err(Error::CertParse(format!(
                "turn binding length prefix {} exceeds remaining {} bytes",
                len,
                bytes.len() - 2
            )));
        }
        let audience = std::str::from_utf8(&bytes[2..2 + len])
            .map_err(|e| Error::CertParse(format!("turn binding audience not utf-8: {e}")))?
            .to_string();
        Ok(Self { audience })
    }
}

/// SHA-256 digest of one bound artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Digest(pub [u8; 32]);

impl Digest {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 32 {
            return Err(Error::InvalidDigestLength(bytes.len()));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(bytes);
        Ok(Self(arr))
    }

    pub fn from_hex(s: &str) -> Result<Self> {
        let bytes = hex::decode(s).map_err(|_| Error::InvalidDigestLength(s.len() / 2))?;
        Self::from_bytes(&bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn hex(&self) -> String {
        hex::encode(self.0)
    }
}

/// Triple bound into every issued SVID. F1's "any digest changes → halt"
/// invariant is enforced by comparing this triple to live digests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DigestTriple {
    pub model: Digest,
    pub manifest: Digest,
    pub config: Digest,
}

impl DigestTriple {
    /// Wire format inside the X.509 extension: model || manifest || config.
    pub fn encode(&self) -> [u8; DIGEST_BINDING_LEN] {
        let mut out = [0u8; DIGEST_BINDING_LEN];
        out[..32].copy_from_slice(&self.model.0);
        out[32..64].copy_from_slice(&self.manifest.0);
        out[64..96].copy_from_slice(&self.config.0);
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != DIGEST_BINDING_LEN {
            return Err(Error::InvalidDigestLength(bytes.len()));
        }
        let model = Digest::from_bytes(&bytes[..32])?;
        let manifest = Digest::from_bytes(&bytes[32..64])?;
        let config = Digest::from_bytes(&bytes[64..96])?;
        Ok(Self {
            model,
            manifest,
            config,
        })
    }
}

/// X.509-SVID issued by the local CA. Holds the leaf cert + private key as
/// PEM strings; consumers deliver them to the runtime as a unit.
#[derive(Debug, Clone)]
pub struct X509Svid {
    pub spiffe_id: SpiffeId,
    pub digests: DigestTriple,
    /// Chat-template binding (per ADR-022 / OCI-B). `Some` when the issuer
    /// was given a chat-template digest; `None` when the SVID was issued
    /// without one (back-compatible — every pre-OCI-B SVID is `None`).
    pub chat_template: Option<Digest>,
    pub cert_pem: String,
    pub key_pem: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn turn_binding_round_trip() {
        let tb = TurnBinding {
            audience: "aegis-turn://session-abc/3".to_string(),
        };
        let encoded = tb.encode().unwrap();
        // 2 bytes length prefix + audience body
        assert_eq!(encoded.len(), 2 + tb.audience.len());
        let decoded = TurnBinding::decode(&encoded).unwrap();
        assert_eq!(decoded, tb);
    }

    #[test]
    fn turn_binding_rejects_empty_audience() {
        let tb = TurnBinding {
            audience: String::new(),
        };
        let err = tb.encode().unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn turn_binding_decode_rejects_short_input() {
        assert!(TurnBinding::decode(&[]).is_err());
        assert!(TurnBinding::decode(&[0]).is_err());
        // Length prefix says 10 bytes but only 1 follows.
        assert!(TurnBinding::decode(&[0, 10, b'x']).is_err());
    }
}
