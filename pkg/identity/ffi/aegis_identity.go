// Package ffi exposes the Rust `aegis-identity` crate's C ABI to Go via
// cgo. Used by the control plane to issue X.509-SVIDs in-process,
// avoiding the latency of shelling out to `aegis identity issue`.
//
// The Rust side is at crates/identity/src/ffi.rs; the C declarations
// the cgo preamble #includes live at pkg/identity/ffi/aegis_identity.h.
//
// Linking:
//
//	cgo searches for libaegis_identity at link time. Set CGO_LDFLAGS or
//	rely on the workspace's `target/release` (or debug) directory. The
//	conformance/Go workflows do `cargo build -p aegis-identity --release`
//	first, then run go test with LD_LIBRARY_PATH pointing at target/release.
package ffi

/*
#cgo CFLAGS: -I${SRCDIR}
#cgo LDFLAGS: -laegis_identity
#include <stdlib.h>
#include "aegis_identity.h"
*/
import "C"

import (
	"errors"
	"fmt"
	"runtime"
	"unsafe"
)

// CA wraps an opaque pointer to a loaded `LocalCa`. Always call Close
// to release the underlying Rust allocation; SetFinalizer handles the
// case where the caller forgets, but explicit Close is preferred.
type CA struct {
	handle *C.AegisLocalCa
}

// Init creates a new CA at `dir` for `trustDomain`. Refuses to overwrite
// an existing CA (matches Rust's LocalCa::init).
func Init(dir, trustDomain string) (*CA, error) {
	cdir := C.CString(dir)
	defer C.free(unsafe.Pointer(cdir))
	ctd := C.CString(trustDomain)
	defer C.free(unsafe.Pointer(ctd))

	handle := C.aegis_identity_ca_init(cdir, ctd)
	if handle == nil {
		return nil, lastError()
	}
	return wrapCA(handle), nil
}

// Load reconstitutes an existing CA from disk.
func Load(dir string) (*CA, error) {
	cdir := C.CString(dir)
	defer C.free(unsafe.Pointer(cdir))

	handle := C.aegis_identity_ca_load(cdir)
	if handle == nil {
		return nil, lastError()
	}
	return wrapCA(handle), nil
}

// Close releases the Rust-side CA allocation. Idempotent.
func (c *CA) Close() {
	if c == nil || c.handle == nil {
		return
	}
	C.aegis_identity_ca_free(c.handle)
	c.handle = nil
	runtime.SetFinalizer(c, nil)
}

func wrapCA(handle *C.AegisLocalCa) *CA {
	c := &CA{handle: handle}
	runtime.SetFinalizer(c, func(c *CA) { c.Close() })
	return c
}

// Digests carries the (model, manifest, config) SHA-256 triple bound
// into every issued SVID per ADR-003 (F1).
type Digests struct {
	Model    [32]byte
	Manifest [32]byte
	Config   [32]byte
}

// SVID is the typed result of a successful Issue. Strings are owned by
// Go; the Rust-side allocations have already been freed by the time
// Issue returns.
type SVID struct {
	CertPEM  string
	KeyPEM   string
	SpiffeID string
}

// Issue stamps a fresh SVID for the given workload + instance, binding
// the digest triple into the cert's custom extension. Safe to call
// concurrently against the same CA (Rust side takes &self).
func (c *CA) Issue(workload, instance string, digests Digests) (*SVID, error) {
	if c == nil || c.handle == nil {
		return nil, errors.New("aegis_identity: Issue called on closed CA")
	}
	cw := C.CString(workload)
	defer C.free(unsafe.Pointer(cw))
	ci := C.CString(instance)
	defer C.free(unsafe.Pointer(ci))

	var out C.AegisSvid
	rc := C.aegis_identity_issue_svid(
		c.handle,
		cw,
		ci,
		(*C.uint8_t)(unsafe.Pointer(&digests.Model[0])),
		(*C.uint8_t)(unsafe.Pointer(&digests.Manifest[0])),
		(*C.uint8_t)(unsafe.Pointer(&digests.Config[0])),
		&out,
	)
	if rc != C.AEGIS_OK {
		return nil, lastError()
	}
	defer C.aegis_identity_svid_clear(&out)

	return &SVID{
		CertPEM:  C.GoString(out.cert_pem),
		KeyPEM:   C.GoString(out.key_pem),
		SpiffeID: C.GoString(out.spiffe_id),
	}, nil
}

func lastError() error {
	cs := C.aegis_identity_last_error()
	if cs == nil {
		return errors.New("aegis_identity: unknown FFI error")
	}
	return fmt.Errorf("aegis_identity: %s", C.GoString(cs))
}
