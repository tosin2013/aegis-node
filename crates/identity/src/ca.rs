//! File-backed local CA for issuing Aegis workload identities.
//!
//! Per ADR-003 (F1) the local CLI ships a built-in lightweight CA so a
//! developer can run `aegis identity init` once and have a working SPIFFE
//! trust domain on disk without standing up SPIRE. Phase 2 swaps this for
//! SPIRE workload attestation; the on-disk artifacts stay user-private.
//!
//! Layout under `<dir>`:
//!
//! ```text
//! ca.crt           PEM root certificate, mode 0644
//! ca.key           PEM PKCS#8 private key, mode 0600
//! trust_domain     plain-text trust domain string
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use rcgen::{
    BasicConstraints, Certificate, CertificateParams, CustomExtension, DnType, Ia5String, IsCa,
    KeyPair, SanType,
};
use time::{Duration, OffsetDateTime};

use crate::error::{Error, Result};
use crate::spiffe::SpiffeId;
use crate::svid::{
    Digest, DigestTriple, X509Svid, CHAT_TEMPLATE_BINDING_LEN, CHAT_TEMPLATE_BINDING_OID,
    DIGEST_BINDING_OID,
};

const CA_CERT_FILE: &str = "ca.crt";
const CA_KEY_FILE: &str = "ca.key";
const TRUST_DOMAIN_FILE: &str = "trust_domain";

const CA_VALIDITY_YEARS: i64 = 10;
const SVID_VALIDITY_HOURS: i64 = 24;

/// File-backed local CA. Holds the issuer cert + key in memory, ready to
/// stamp leaf SVIDs.
pub struct LocalCa {
    dir: PathBuf,
    trust_domain: String,
    ca_cert: Certificate,
    ca_key: KeyPair,
}

impl std::fmt::Debug for LocalCa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalCa")
            .field("dir", &self.dir)
            .field("trust_domain", &self.trust_domain)
            .finish_non_exhaustive()
    }
}

impl LocalCa {
    /// First-time setup. Creates `dir`, generates a fresh CA, persists it.
    /// Refuses to overwrite an existing CA — re-init would silently break
    /// every previously issued SVID.
    pub fn init<P: AsRef<Path>>(dir: P, trust_domain: &str) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        let cert_path = dir.join(CA_CERT_FILE);
        if cert_path.exists() {
            return Err(Error::CaAlreadyInitialized(dir.display().to_string()));
        }
        validate_trust_domain_for_ca(trust_domain)?;
        fs::create_dir_all(&dir)?;

        let now = OffsetDateTime::now_utc();
        let mut params = CertificateParams::default();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.not_before = now - Duration::minutes(5);
        params.not_after = now + Duration::days(365 * CA_VALIDITY_YEARS);
        params
            .distinguished_name
            .push(DnType::CommonName, "Aegis-Node Local CA");
        params.distinguished_name.push(
            DnType::OrganizationName,
            format!("Aegis-Node trust domain {trust_domain}"),
        );

        let ca_key = KeyPair::generate()?;
        let ca_cert = params.self_signed(&ca_key)?;

        write_file(&cert_path, ca_cert.pem().as_bytes(), 0o644)?;
        write_file(
            &dir.join(CA_KEY_FILE),
            ca_key.serialize_pem().as_bytes(),
            0o600,
        )?;
        write_file(&dir.join(TRUST_DOMAIN_FILE), trust_domain.as_bytes(), 0o644)?;

        Ok(Self {
            dir,
            trust_domain: trust_domain.to_string(),
            ca_cert,
            ca_key,
        })
    }

    /// Load an existing CA from disk. The cert is reconstituted from PEM and
    /// re-signed in memory with the loaded key — issued leaves chain back via
    /// issuer DN + key, so the on-disk PEM is the authority.
    pub fn load<P: AsRef<Path>>(dir: P) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        let cert_path = dir.join(CA_CERT_FILE);
        let key_path = dir.join(CA_KEY_FILE);
        let td_path = dir.join(TRUST_DOMAIN_FILE);
        if !cert_path.exists() || !key_path.exists() || !td_path.exists() {
            return Err(Error::CaNotInitialized(dir.display().to_string()));
        }

        let ca_pem = fs::read_to_string(&cert_path)?;
        let key_pem = fs::read_to_string(&key_path)?;
        let trust_domain = fs::read_to_string(&td_path)?.trim().to_string();
        validate_trust_domain_for_ca(&trust_domain)?;

        let ca_key = KeyPair::from_pem(&key_pem)?;
        let ca_params = CertificateParams::from_ca_cert_pem(&ca_pem)?;
        let ca_cert = ca_params.self_signed(&ca_key)?;

        Ok(Self {
            dir,
            trust_domain,
            ca_cert,
            ca_key,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn trust_domain(&self) -> &str {
        &self.trust_domain
    }

    /// Issue a fresh X.509-SVID for the named workload + instance, binding
    /// the (model, manifest, config) digest triple into a custom extension.
    /// No chat-template binding — equivalent to
    /// [`Self::issue_svid_with_chat_template`] passing `None`. Kept as the
    /// primary entry point so pre-OCI-B callers don't need to thread an
    /// `Option`.
    pub fn issue_svid(
        &self,
        workload_name: &str,
        instance: &str,
        digests: DigestTriple,
    ) -> Result<X509Svid> {
        self.issue_svid_with_chat_template(workload_name, instance, digests, None)
    }

    /// Issue an SVID and (when `Some`) attach a second non-critical
    /// extension carrying the chat-template SHA-256 from
    /// [`CHAT_TEMPLATE_BINDING_OID`] (per ADR-022 / OCI-B). The `(model,
    /// manifest, config)` triple is always bound; the chat-template is
    /// only bound when supplied.
    ///
    /// We use a *separate* extension rather than extending
    /// [`DigestTriple`] because the Compatibility Charter freezes the
    /// digest-binding payload format at 96 bytes. Adding a fourth
    /// digest there would break every previously-issued SVID's parser.
    /// A new optional extension is back-compatible: pre-OCI-B SVIDs
    /// simply lack it, and verifiers treat absence as "no
    /// chat-template binding."
    pub fn issue_svid_with_chat_template(
        &self,
        workload_name: &str,
        instance: &str,
        digests: DigestTriple,
        chat_template: Option<Digest>,
    ) -> Result<X509Svid> {
        let spiffe_id = SpiffeId::new(&self.trust_domain, workload_name, instance)?;
        let now = OffsetDateTime::now_utc();

        let mut params = CertificateParams::default();
        params.not_before = now - Duration::minutes(5);
        params.not_after = now + Duration::hours(SVID_VALIDITY_HOURS);
        params
            .distinguished_name
            .push(DnType::CommonName, workload_name);
        params.subject_alt_names = vec![SanType::URI(
            Ia5String::try_from(spiffe_id.uri())
                .map_err(|e| Error::CertParse(format!("SPIFFE URI not IA5-encodable: {e}")))?,
        )];

        let mut ext =
            CustomExtension::from_oid_content(DIGEST_BINDING_OID, digests.encode().to_vec());
        // Non-critical so the SVID can be presented as a server/client
        // cert through standard TLS libraries (rustls' webpki rejects
        // any unknown critical extension per RFC 5280). The runtime
        // validates the binding via `verify_digest_binding` regardless
        // of the criticality flag, so the security guarantee is
        // preserved at the application layer.
        ext.set_criticality(false);
        params.custom_extensions.push(ext);

        if let Some(template) = chat_template {
            let mut ct_ext =
                CustomExtension::from_oid_content(CHAT_TEMPLATE_BINDING_OID, template.0.to_vec());
            ct_ext.set_criticality(false);
            params.custom_extensions.push(ct_ext);
        }

        let leaf_key = KeyPair::generate()?;
        let leaf = params.signed_by(&leaf_key, &self.ca_cert, &self.ca_key)?;

        Ok(X509Svid {
            spiffe_id,
            digests,
            chat_template,
            cert_pem: leaf.pem(),
            key_pem: leaf_key.serialize_pem(),
        })
    }

    /// PEM of the CA root certificate. Useful for trust-bundle distribution.
    pub fn root_cert_pem(&self) -> String {
        self.ca_cert.pem()
    }
}

fn validate_trust_domain_for_ca(td: &str) -> Result<()> {
    // Reuse SpiffeId's validator by attempting to construct a sentinel ID.
    SpiffeId::new(td, "ca", "root").map(|_| ())
}

#[cfg(unix)]
fn write_file(path: &Path, contents: &[u8], mode: u32) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut opts = std::fs::OpenOptions::new();
    opts.create_new(true).write(true).mode(mode);
    let mut f = opts.open(path)?;
    use std::io::Write;
    f.write_all(contents)?;
    f.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn write_file(path: &Path, contents: &[u8], _mode: u32) -> Result<()> {
    let mut opts = std::fs::OpenOptions::new();
    opts.create_new(true).write(true);
    let mut f = opts.open(path)?;
    use std::io::Write;
    f.write_all(contents)?;
    f.sync_all()?;
    Ok(())
}

/// Helper used by issued-cert verifiers (and the future `aegis identity verify`
/// command) to extract the digest triple from an X.509-SVID PEM. The cert
/// must contain the digest-binding extension or this returns an error.
pub fn extract_digest_triple_from_pem(cert_pem: &str) -> Result<DigestTriple> {
    use x509_parser::parse_x509_certificate;
    use x509_parser::pem::Pem;

    let pem = Pem::iter_from_buffer(cert_pem.as_bytes())
        .next()
        .ok_or_else(|| Error::CertParse("no PEM block".to_string()))?
        .map_err(|e| Error::CertParse(e.to_string()))?;
    let (_, cert) =
        parse_x509_certificate(&pem.contents).map_err(|e| Error::CertParse(e.to_string()))?;

    let oid_str = oid_components_to_dotted(DIGEST_BINDING_OID);
    let ext = cert
        .extensions()
        .iter()
        .find(|e| e.oid.to_id_string() == oid_str)
        .ok_or_else(|| Error::CertParse(format!("missing digest-binding extension {oid_str}")))?;

    DigestTriple::decode(ext.value)
}

/// Extract the chat-template binding from an SVID PEM, if present. Per
/// ADR-022 / OCI-B this extension is **optional** — `Ok(None)` means the
/// SVID was issued without a chat-template digest (back-compatible with
/// every pre-OCI-B SVID). `Ok(Some(digest))` returns the bound 32-byte
/// SHA-256. `Err` only for cert-format problems or a malformed payload.
pub fn extract_chat_template_from_pem(cert_pem: &str) -> Result<Option<Digest>> {
    use x509_parser::parse_x509_certificate;
    use x509_parser::pem::Pem;

    let pem = Pem::iter_from_buffer(cert_pem.as_bytes())
        .next()
        .ok_or_else(|| Error::CertParse("no PEM block".to_string()))?
        .map_err(|e| Error::CertParse(e.to_string()))?;
    let (_, cert) =
        parse_x509_certificate(&pem.contents).map_err(|e| Error::CertParse(e.to_string()))?;

    let oid_str = oid_components_to_dotted(CHAT_TEMPLATE_BINDING_OID);
    match cert
        .extensions()
        .iter()
        .find(|e| e.oid.to_id_string() == oid_str)
    {
        Some(ext) => {
            if ext.value.len() != CHAT_TEMPLATE_BINDING_LEN {
                return Err(Error::CertParse(format!(
                    "chat-template binding extension has wrong length: expected {}, got {}",
                    CHAT_TEMPLATE_BINDING_LEN,
                    ext.value.len()
                )));
            }
            Ok(Some(Digest::from_bytes(ext.value)?))
        }
        None => Ok(None),
    }
}

/// Like `extract_digest_triple_from_pem` but for the SPIFFE ID encoded in the
/// leaf cert's URI SAN.
pub fn extract_spiffe_id_from_pem(cert_pem: &str) -> Result<SpiffeId> {
    use x509_parser::extensions::{GeneralName, ParsedExtension};
    use x509_parser::parse_x509_certificate;
    use x509_parser::pem::Pem;

    let pem = Pem::iter_from_buffer(cert_pem.as_bytes())
        .next()
        .ok_or_else(|| Error::CertParse("no PEM block".to_string()))?
        .map_err(|e| Error::CertParse(e.to_string()))?;
    let (_, cert) =
        parse_x509_certificate(&pem.contents).map_err(|e| Error::CertParse(e.to_string()))?;

    for ext in cert.extensions() {
        if let ParsedExtension::SubjectAlternativeName(san) = ext.parsed_extension() {
            for name in &san.general_names {
                if let GeneralName::URI(uri) = name {
                    return SpiffeId::parse(uri);
                }
            }
        }
    }
    Err(Error::CertParse("no URI SAN found".to_string()))
}

fn oid_components_to_dotted(parts: &[u64]) -> String {
    parts
        .iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(".")
}
