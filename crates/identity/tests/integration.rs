//! End-to-end tests for the local CA and X.509-SVID issuance.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use aegis_identity::{
    extract_digest_triple_from_pem, extract_spiffe_id_from_pem, Digest, DigestTriple, Error,
    LocalCa, SpiffeId,
};

const TRUST_DOMAIN: &str = "aegis-node.local";

fn test_digests() -> DigestTriple {
    DigestTriple {
        model: Digest([0xAAu8; 32]),
        manifest: Digest([0xBBu8; 32]),
        config: Digest([0xCCu8; 32]),
    }
}

#[test]
fn init_then_load_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let ca = LocalCa::init(dir.path(), TRUST_DOMAIN).unwrap();
    assert_eq!(ca.trust_domain(), TRUST_DOMAIN);
    drop(ca);

    let loaded = LocalCa::load(dir.path()).unwrap();
    assert_eq!(loaded.trust_domain(), TRUST_DOMAIN);
}

#[test]
fn double_init_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    LocalCa::init(dir.path(), TRUST_DOMAIN).unwrap();
    let err = LocalCa::init(dir.path(), TRUST_DOMAIN).unwrap_err();
    assert!(matches!(err, Error::CaAlreadyInitialized(_)));
}

#[test]
fn load_uninitialized_dir_fails() {
    let dir = tempfile::tempdir().unwrap();
    let err = LocalCa::load(dir.path()).unwrap_err();
    assert!(matches!(err, Error::CaNotInitialized(_)));
}

#[test]
fn issued_svid_carries_spiffe_id_and_digests() {
    let dir = tempfile::tempdir().unwrap();
    let ca = LocalCa::init(dir.path(), TRUST_DOMAIN).unwrap();
    let digests = test_digests();

    let svid = ca.issue_svid("research", "inst-001", digests).unwrap();

    let expected = SpiffeId::new(TRUST_DOMAIN, "research", "inst-001").unwrap();
    assert_eq!(svid.spiffe_id, expected);

    let parsed_id = extract_spiffe_id_from_pem(&svid.cert_pem).unwrap();
    assert_eq!(parsed_id, expected);

    let parsed_digests = extract_digest_triple_from_pem(&svid.cert_pem).unwrap();
    assert_eq!(parsed_digests, digests);
}

#[test]
fn separate_issuances_use_independent_keys() {
    let dir = tempfile::tempdir().unwrap();
    let ca = LocalCa::init(dir.path(), TRUST_DOMAIN).unwrap();
    let digests = test_digests();
    let a = ca.issue_svid("research", "inst-001", digests).unwrap();
    let b = ca.issue_svid("research", "inst-002", digests).unwrap();
    assert_ne!(a.key_pem, b.key_pem);
    assert_ne!(a.cert_pem, b.cert_pem);
}

#[test]
fn loaded_ca_can_issue_svids() {
    let dir = tempfile::tempdir().unwrap();
    {
        LocalCa::init(dir.path(), TRUST_DOMAIN).unwrap();
    }
    let ca = LocalCa::load(dir.path()).unwrap();
    let svid = ca
        .issue_svid("research", "inst-load", test_digests())
        .unwrap();
    let parsed = extract_spiffe_id_from_pem(&svid.cert_pem).unwrap();
    assert_eq!(parsed.trust_domain(), TRUST_DOMAIN);
    assert_eq!(parsed.workload_name(), "research");
    assert_eq!(parsed.instance(), "inst-load");
}

#[test]
fn rejects_invalid_workload_name() {
    let dir = tempfile::tempdir().unwrap();
    let ca = LocalCa::init(dir.path(), TRUST_DOMAIN).unwrap();
    let err = ca
        .issue_svid("bad workload", "inst-001", test_digests())
        .unwrap_err();
    assert!(matches!(err, Error::InvalidSpiffeId { .. }));
}

#[cfg(unix)]
#[test]
fn ca_key_file_has_owner_only_permissions() {
    use std::os::unix::fs::MetadataExt;

    let dir = tempfile::tempdir().unwrap();
    LocalCa::init(dir.path(), TRUST_DOMAIN).unwrap();
    let key_meta = std::fs::metadata(dir.path().join("ca.key")).unwrap();
    let mode = key_meta.mode() & 0o777;
    assert_eq!(mode, 0o600, "ca.key must be 0600 (got {mode:o})");
}
