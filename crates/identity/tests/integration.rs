//! End-to-end tests for the local CA and X.509-SVID issuance.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use aegis_identity::{
    extract_chat_template_from_pem, extract_digest_triple_from_pem, extract_spiffe_id_from_pem,
    verify_chat_template_binding, verify_digest_binding, Digest, DigestField, DigestTriple, Error,
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

#[test]
fn issue_svid_without_chat_template_omits_extension() {
    // Default `issue_svid` MUST NOT attach the chat-template extension —
    // every pre-OCI-B SVID, and every non-GGUF model session, follows
    // this path. Verifies back-compat with the Compatibility Charter
    // freeze on the 96-byte digest-binding payload.
    let dir = tempfile::tempdir().unwrap();
    let ca = LocalCa::init(dir.path(), TRUST_DOMAIN).unwrap();
    let svid = ca
        .issue_svid("research", "inst-001", test_digests())
        .unwrap();

    assert_eq!(svid.chat_template, None, "X509Svid should report None");
    let parsed = extract_chat_template_from_pem(&svid.cert_pem).unwrap();
    assert_eq!(parsed, None, "extension should be absent");
    // verify_chat_template_binding with no live and no bound → match.
    assert!(verify_chat_template_binding(&svid.cert_pem, None)
        .unwrap()
        .is_none());
}

#[test]
fn issue_svid_with_chat_template_binds_extension_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let ca = LocalCa::init(dir.path(), TRUST_DOMAIN).unwrap();
    let template = Digest([0x42u8; 32]);
    let svid = ca
        .issue_svid_with_chat_template("research", "inst-001", test_digests(), Some(template))
        .unwrap();

    assert_eq!(svid.chat_template, Some(template));
    let parsed = extract_chat_template_from_pem(&svid.cert_pem).unwrap();
    assert_eq!(parsed, Some(template));
    // The (model, manifest, config) triple must still be present
    // alongside the new extension — the two extensions don't collide.
    let parsed_triple = extract_digest_triple_from_pem(&svid.cert_pem).unwrap();
    assert_eq!(parsed_triple, test_digests());
}

#[test]
fn verify_chat_template_binding_signals_drift() {
    let dir = tempfile::tempdir().unwrap();
    let ca = LocalCa::init(dir.path(), TRUST_DOMAIN).unwrap();
    let bound = Digest([0x11u8; 32]);
    let svid = ca
        .issue_svid_with_chat_template("research", "inst-001", test_digests(), Some(bound))
        .unwrap();

    // Match: bound and live agree.
    assert!(verify_chat_template_binding(&svid.cert_pem, Some(&bound))
        .unwrap()
        .is_none());

    // Drift: live differs from bound → DigestMismatch on the
    // ChatTemplate field, regardless of whether the (model, manifest,
    // config) triple still matches.
    let drifted = Digest([0x22u8; 32]);
    let mismatch = verify_chat_template_binding(&svid.cert_pem, Some(&drifted))
        .unwrap()
        .expect("should detect drift");
    assert_eq!(mismatch.field, DigestField::ChatTemplate);
    assert_eq!(mismatch.bound, bound);
    assert_eq!(mismatch.live, drifted);

    // SVID claims a binding, but the runtime supplied no live digest —
    // unsafe ambiguity, refuse.
    let none_live = verify_chat_template_binding(&svid.cert_pem, None)
        .unwrap()
        .expect("should refuse missing-live ambiguity");
    assert_eq!(none_live.field, DigestField::ChatTemplate);
    assert_eq!(none_live.bound, bound);

    // The (model, manifest, config) verifier is unaffected — chat-
    // template lives in a separate extension.
    assert!(verify_digest_binding(&svid.cert_pem, &test_digests())
        .unwrap()
        .is_none());
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
