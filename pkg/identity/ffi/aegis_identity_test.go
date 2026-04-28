package ffi

import (
	"bytes"
	"crypto/x509"
	"encoding/asn1"
	"encoding/pem"
	"net/url"
	"os"
	"testing"
)

// digestBindingOID — placeholder OID matching crates/identity/src/svid.rs
// (slated for replacement when we register a real OID; the format is
// what the Compatibility Charter freezes, not the OID itself).
var digestBindingOID = asn1.ObjectIdentifier{1, 3, 6, 1, 4, 1, 99999, 1}

func TestFFI_RoundTrip(t *testing.T) {
	dir, err := os.MkdirTemp("", "aegis-ffi-")
	if err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	defer os.RemoveAll(dir)

	ca, err := Init(dir, "ffi-test.local")
	if err != nil {
		t.Fatalf("Init: %v", err)
	}
	defer ca.Close()

	want := Digests{}
	for i := range want.Model {
		want.Model[i] = 0xAA
		want.Manifest[i] = 0xBB
		want.Config[i] = 0xCC
	}

	svid, err := ca.Issue("research", "inst-001", want)
	if err != nil {
		t.Fatalf("Issue: %v", err)
	}

	// Cert PEM must parse and carry the SPIFFE URI in a SAN entry.
	block, _ := pem.Decode([]byte(svid.CertPEM))
	if block == nil {
		t.Fatal("cert PEM did not decode")
	}
	cert, err := x509.ParseCertificate(block.Bytes)
	if err != nil {
		t.Fatalf("parse cert: %v", err)
	}

	wantURI := "spiffe://ffi-test.local/agent/research/inst-001"
	if svid.SpiffeID != wantURI {
		t.Errorf("returned SpiffeID: got %q want %q", svid.SpiffeID, wantURI)
	}
	if !certHasURISAN(cert, wantURI) {
		t.Errorf("URI SAN %q not found in cert", wantURI)
	}

	// Digest extension: 96 raw bytes, model||manifest||config.
	gotDigest := findExtension(cert, digestBindingOID)
	if gotDigest == nil {
		t.Fatalf("digest binding extension OID %v not found", digestBindingOID)
	}
	if len(gotDigest) != 96 {
		t.Fatalf("digest binding length: got %d want 96", len(gotDigest))
	}
	if !bytes.Equal(gotDigest[:32], want.Model[:]) {
		t.Errorf("model digest mismatch")
	}
	if !bytes.Equal(gotDigest[32:64], want.Manifest[:]) {
		t.Errorf("manifest digest mismatch")
	}
	if !bytes.Equal(gotDigest[64:96], want.Config[:]) {
		t.Errorf("config digest mismatch")
	}

	// Key PEM should also decode; we don't need the key value, just the format.
	if blk, _ := pem.Decode([]byte(svid.KeyPEM)); blk == nil {
		t.Error("key PEM did not decode")
	}
}

func TestFFI_LoadAfterInit(t *testing.T) {
	dir, err := os.MkdirTemp("", "aegis-ffi-load-")
	if err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	defer os.RemoveAll(dir)

	caInit, err := Init(dir, "ffi-test.local")
	if err != nil {
		t.Fatalf("Init: %v", err)
	}
	caInit.Close()

	caLoad, err := Load(dir)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	defer caLoad.Close()

	digests := Digests{}
	for i := range digests.Model {
		digests.Model[i] = 0x42
	}
	if _, err := caLoad.Issue("research", "inst-load", digests); err != nil {
		t.Fatalf("Issue after Load: %v", err)
	}
}

func TestFFI_LoadMissingDir(t *testing.T) {
	if _, err := Load("/nonexistent/aegis-ffi-load-test"); err == nil {
		t.Fatal("expected error loading from missing dir")
	}
}

func TestFFI_DoubleInitFails(t *testing.T) {
	dir, err := os.MkdirTemp("", "aegis-ffi-double-")
	if err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	defer os.RemoveAll(dir)

	first, err := Init(dir, "ffi-test.local")
	if err != nil {
		t.Fatalf("first Init: %v", err)
	}
	first.Close()

	if _, err := Init(dir, "ffi-test.local"); err == nil {
		t.Fatal("expected second Init to fail")
	}
}

func certHasURISAN(cert *x509.Certificate, want string) bool {
	wantURL, err := url.Parse(want)
	if err != nil {
		return false
	}
	for _, u := range cert.URIs {
		if u.String() == wantURL.String() {
			return true
		}
	}
	return false
}

// findExtension returns the raw extension bytes (the inner content) for
// the given OID, or nil if the cert doesn't carry it.
func findExtension(cert *x509.Certificate, oid asn1.ObjectIdentifier) []byte {
	for _, ext := range cert.Extensions {
		if ext.Id.Equal(oid) {
			return ext.Value
		}
	}
	return nil
}
