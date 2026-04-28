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
    pub cert_pem: String,
    pub key_pem: String,
}
