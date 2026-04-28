//! C ABI for `aegis-identity` — used by the Go control plane to issue
//! workload identity SVIDs in-process (issue #17).
//!
//! The header at `pkg/identity/ffi/aegis_identity.h` is the canonical
//! C-side declaration; this module's `#[no_mangle] extern "C"` functions
//! must match it. A drift between the two surfaces as a Go cgo build
//! error.
//!
//! ## Memory ownership rules
//!
//! - `aegis_identity_ca_init` / `aegis_identity_ca_load` return an opaque
//!   pointer the caller MUST eventually free with
//!   `aegis_identity_ca_free`.
//! - `aegis_identity_issue_svid` populates the caller-provided `out_svid`
//!   with three heap-allocated `char*` strings. The caller MUST call
//!   `aegis_identity_svid_clear` to release them.
//! - `aegis_identity_last_error` returns a pointer into thread-local
//!   storage. The caller MUST NOT free it; the lifetime ends at the
//!   next FFI call from the same thread.
//!
//! ## Thread safety
//!
//! `LocalCa::issue_svid` takes `&self`, so concurrent issuance against
//! the same CA pointer is sound. `LAST_ERROR` is per-thread, so error
//! reads are race-free as long as each caller reads on the same thread
//! that issued the failing call.

use std::cell::RefCell;
use std::ffi::{c_char, c_int, CStr, CString};
use std::path::Path;
use std::ptr;

use crate::svid::{Digest, DigestTriple};
use crate::LocalCa;

pub const AEGIS_OK: c_int = 0;
pub const AEGIS_ERR_NULL: c_int = -1;
pub const AEGIS_ERR_INVALID_UTF8: c_int = -2;
pub const AEGIS_ERR_LOAD: c_int = -3;
pub const AEGIS_ERR_ISSUE: c_int = -4;
pub const AEGIS_ERR_INTERNAL: c_int = -5;

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_last_error(msg: impl Into<String>) {
    let s = msg.into();
    let cs = CString::new(s).unwrap_or_else(|_| {
        // The only way CString::new fails is interior NULs — replace with
        // a generic message so the caller still gets *something*.
        CString::new("aegis_identity: error message contained NULs").unwrap_or_default()
    });
    LAST_ERROR.with(|e| *e.borrow_mut() = Some(cs));
}

/// Returns the last error from the calling thread, or NULL if none.
/// The pointer is owned by Rust thread-local storage; do NOT free it.
#[no_mangle]
pub extern "C" fn aegis_identity_last_error() -> *const c_char {
    LAST_ERROR.with(|e| {
        e.borrow()
            .as_ref()
            .map(|cs| cs.as_ptr())
            .unwrap_or(ptr::null())
    })
}

/// Initialize a new CA at `dir` for `trust_domain`. Refuses to overwrite
/// an existing CA (per `LocalCa::init`).
///
/// # Safety
///
/// `dir` and `trust_domain` must be NUL-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn aegis_identity_ca_init(
    dir: *const c_char,
    trust_domain: *const c_char,
) -> *mut LocalCa {
    let Some(dir) = (unsafe { c_str(dir, "dir") }) else {
        return ptr::null_mut();
    };
    let Some(td) = (unsafe { c_str(trust_domain, "trust_domain") }) else {
        return ptr::null_mut();
    };
    match LocalCa::init(Path::new(dir), td) {
        Ok(ca) => Box::into_raw(Box::new(ca)),
        Err(e) => {
            set_last_error(format!("init CA: {e}"));
            ptr::null_mut()
        }
    }
}

/// Load an existing CA from `dir`.
///
/// # Safety
///
/// `dir` must be a NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn aegis_identity_ca_load(dir: *const c_char) -> *mut LocalCa {
    let Some(dir) = (unsafe { c_str(dir, "dir") }) else {
        return ptr::null_mut();
    };
    match LocalCa::load(Path::new(dir)) {
        Ok(ca) => Box::into_raw(Box::new(ca)),
        Err(e) => {
            set_last_error(format!("load CA: {e}"));
            ptr::null_mut()
        }
    }
}

/// Free a CA handle returned by `ca_init` or `ca_load`. Safe to call
/// with NULL (no-op).
///
/// # Safety
///
/// `ca` must have come from `aegis_identity_ca_init` or
/// `aegis_identity_ca_load` and must not have been previously freed.
#[no_mangle]
pub unsafe extern "C" fn aegis_identity_ca_free(ca: *mut LocalCa) {
    if !ca.is_null() {
        drop(unsafe { Box::from_raw(ca) });
    }
}

/// Output struct for `issue_svid`. All three pointers are heap-allocated
/// NUL-terminated UTF-8 strings, freed via `aegis_identity_svid_clear`.
#[repr(C)]
pub struct AegisSvid {
    pub cert_pem: *mut c_char,
    pub key_pem: *mut c_char,
    pub spiffe_id: *mut c_char,
}

/// Issue an X.509-SVID. Digest pointers must each point to 32 bytes.
///
/// Returns `AEGIS_OK` on success and populates `*out_svid`. On failure
/// returns a negative code; the caller can retrieve a human-readable
/// message via `aegis_identity_last_error`.
///
/// # Safety
///
/// All pointers must be valid for the duration of the call.
/// `model_digest`, `manifest_digest`, `config_digest` must each point
/// to 32 readable bytes. `workload` and `instance` must be
/// NUL-terminated UTF-8. `out_svid` must point to a writable
/// `AegisSvid`.
#[no_mangle]
pub unsafe extern "C" fn aegis_identity_issue_svid(
    ca: *const LocalCa,
    workload: *const c_char,
    instance: *const c_char,
    model_digest: *const u8,
    manifest_digest: *const u8,
    config_digest: *const u8,
    out_svid: *mut AegisSvid,
) -> c_int {
    if ca.is_null()
        || workload.is_null()
        || instance.is_null()
        || model_digest.is_null()
        || manifest_digest.is_null()
        || config_digest.is_null()
        || out_svid.is_null()
    {
        set_last_error("null argument");
        return AEGIS_ERR_NULL;
    }

    let Some(workload) = (unsafe { c_str(workload, "workload") }) else {
        return AEGIS_ERR_INVALID_UTF8;
    };
    let Some(instance) = (unsafe { c_str(instance, "instance") }) else {
        return AEGIS_ERR_INVALID_UTF8;
    };

    let triple = unsafe {
        DigestTriple {
            model: read_digest(model_digest),
            manifest: read_digest(manifest_digest),
            config: read_digest(config_digest),
        }
    };

    let svid = match unsafe { &*ca }.issue_svid(workload, instance, triple) {
        Ok(s) => s,
        Err(e) => {
            set_last_error(format!("issue: {e}"));
            return AEGIS_ERR_ISSUE;
        }
    };

    let cert = match CString::new(svid.cert_pem) {
        Ok(c) => c.into_raw(),
        Err(e) => {
            set_last_error(format!("cert PEM contained NULs: {e}"));
            return AEGIS_ERR_INTERNAL;
        }
    };
    let key = match CString::new(svid.key_pem) {
        Ok(c) => c.into_raw(),
        Err(e) => {
            unsafe { drop(CString::from_raw(cert)) };
            set_last_error(format!("key PEM contained NULs: {e}"));
            return AEGIS_ERR_INTERNAL;
        }
    };
    let sid = match CString::new(svid.spiffe_id.uri()) {
        Ok(c) => c.into_raw(),
        Err(e) => {
            unsafe {
                drop(CString::from_raw(cert));
                drop(CString::from_raw(key));
            }
            set_last_error(format!("spiffe id contained NULs: {e}"));
            return AEGIS_ERR_INTERNAL;
        }
    };

    unsafe {
        *out_svid = AegisSvid {
            cert_pem: cert,
            key_pem: key,
            spiffe_id: sid,
        };
    }
    AEGIS_OK
}

/// Free the strings inside an `AegisSvid`. Sets each pointer back to
/// NULL so a second call is a no-op. The caller still owns the
/// `AegisSvid` struct itself.
///
/// # Safety
///
/// `svid` must point to a valid `AegisSvid` previously populated by
/// `aegis_identity_issue_svid`. Must not be called twice on the same
/// pointers without an intervening successful issue.
#[no_mangle]
pub unsafe extern "C" fn aegis_identity_svid_clear(svid: *mut AegisSvid) {
    if svid.is_null() {
        return;
    }
    let s = unsafe { &mut *svid };
    if !s.cert_pem.is_null() {
        unsafe { drop(CString::from_raw(s.cert_pem)) };
        s.cert_pem = ptr::null_mut();
    }
    if !s.key_pem.is_null() {
        unsafe { drop(CString::from_raw(s.key_pem)) };
        s.key_pem = ptr::null_mut();
    }
    if !s.spiffe_id.is_null() {
        unsafe { drop(CString::from_raw(s.spiffe_id)) };
        s.spiffe_id = ptr::null_mut();
    }
}

unsafe fn c_str<'a>(p: *const c_char, label: &'static str) -> Option<&'a str> {
    if p.is_null() {
        set_last_error(format!("{label} is NULL"));
        return None;
    }
    match unsafe { CStr::from_ptr(p) }.to_str() {
        Ok(s) => Some(s),
        Err(e) => {
            set_last_error(format!("{label} not valid UTF-8: {e}"));
            None
        }
    }
}

unsafe fn read_digest(p: *const u8) -> Digest {
    let mut buf = [0u8; 32];
    unsafe { std::ptr::copy_nonoverlapping(p, buf.as_mut_ptr(), 32) };
    Digest(buf)
}
