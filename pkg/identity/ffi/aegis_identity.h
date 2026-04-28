/*
 * aegis_identity.h — C ABI for the Rust `aegis-identity` crate.
 *
 * Match this header to crates/identity/src/ffi.rs. A drift between the
 * two surfaces as a Go cgo build error.
 *
 * Memory ownership rules:
 *
 *   - aegis_identity_ca_init / aegis_identity_ca_load return an opaque
 *     pointer the caller MUST eventually pass to aegis_identity_ca_free.
 *   - aegis_identity_issue_svid populates the caller-provided AegisSvid
 *     with three heap-allocated NUL-terminated strings. Caller MUST
 *     call aegis_identity_svid_clear to release them.
 *   - aegis_identity_last_error returns a pointer into thread-local
 *     storage. Caller MUST NOT free; lifetime ends at the next FFI
 *     call from the same thread.
 *
 * Thread safety:
 *
 *   LocalCa::issue_svid takes &self in Rust, so concurrent issuance
 *   against the same CA pointer is sound. last_error is per-thread.
 */

#ifndef AEGIS_IDENTITY_H
#define AEGIS_IDENTITY_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque handle for a loaded local CA. */
typedef struct AegisLocalCa AegisLocalCa;

typedef struct {
    char* cert_pem;   /* free via aegis_identity_svid_clear */
    char* key_pem;    /* free via aegis_identity_svid_clear */
    char* spiffe_id;  /* free via aegis_identity_svid_clear */
} AegisSvid;

#define AEGIS_OK              0
#define AEGIS_ERR_NULL        (-1)
#define AEGIS_ERR_INVALID_UTF8 (-2)
#define AEGIS_ERR_LOAD        (-3)
#define AEGIS_ERR_ISSUE       (-4)
#define AEGIS_ERR_INTERNAL    (-5)

const char* aegis_identity_last_error(void);

AegisLocalCa* aegis_identity_ca_init(const char* dir, const char* trust_domain);
AegisLocalCa* aegis_identity_ca_load(const char* dir);
void          aegis_identity_ca_free(AegisLocalCa* ca);

int aegis_identity_issue_svid(
    const AegisLocalCa* ca,
    const char* workload,
    const char* instance,
    const uint8_t* model_digest,    /* 32 bytes */
    const uint8_t* manifest_digest, /* 32 bytes */
    const uint8_t* config_digest,   /* 32 bytes */
    AegisSvid* out_svid
);

void aegis_identity_svid_clear(AegisSvid* svid);

#ifdef __cplusplus
}
#endif

#endif /* AEGIS_IDENTITY_H */
