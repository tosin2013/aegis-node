# 11. Hash-Chained, Append-Only Tamper-Evident Ledger

**Status:** Accepted
**Date:** 2026-04-27
**Domain:** Security / Audit Integrity (F9)

## Context

Audit logs are useless if they can be edited after the fact. The compliance question "can the logs be altered?" must have a structural answer — not "we trust the operator," not "we have ACLs," but a cryptographic guarantee that any retroactive modification is immediately detectable.

The same problem has been solved by certificate transparency logs, blockchain systems, and Git itself: hash-chain every entry so that altering any historical record breaks the chain at every subsequent entry.

## Decision

The Trajectory Ledger is a hash-chained, append-only data structure, not a log file. Properties:

1. **Each entry contains:** content payload, timestamp, agent identity hash (F1), and the SHA-256 hash of the previous entry. The genesis entry's "previous" is a session-bound zero value.
2. **Append-only at the API level.** No delete or update operations are exposed. The runtime refuses to bind to a ledger file with non-append-only filesystem permissions where the OS supports it.
3. **`aegis verify` CLI command.** Walks the chain, verifies every hash link, and reports the position of any break. Exit code is non-zero on integrity failure for CI/CD use.
4. **Notarization-ready.** The ledger root hash (the latest entry's hash) can be exported and registered with an external trusted timestamp authority (RFC 3161 TSA) for legal admissibility. Local-only deployments use a built-in TSA-equivalent for development; production deployments integrate with the operator's TSA.
5. **JSON-LD format.** Compatible with W3C Verifiable Credentials, enabling integration with enterprise identity systems and downstream cryptographic verification.

## Consequences

**Positive:**
- Direct, structural answer to "can logs be altered?" — yes they can be altered, but the alteration is provably detectable.
- Notarization integration brings legal admissibility for downstream legal proceedings (incident response, breach notification, contractual disputes).
- Same data structure used for trajectory entries, access entries, approval events, and violation events — single integrity guarantee covers everything.

**Negative:**
- Hashing is not free; per-entry cost is small but non-zero. Throughput-sensitive use cases need batching strategies that don't break the per-entry guarantee.
- An entry that is itself compromised at write time (e.g., an attacker controls the runtime) still produces a valid chain — the chain proves immutability after recording, not honesty at recording. Defense-in-depth (TEE attestation, F1 identity binding) addresses this in the Sovereign tier.
- Append-only filesystem semantics differ across platforms; the runtime must handle each gracefully.

## Domain Considerations

The pattern is exactly the structure of Git commits, Bitcoin block headers, and RFC 6962 certificate transparency logs. Reviewers recognize the model and trust it.

## Implementation Plan

1. Define the JSON-LD ledger entry schema (`@context`, content fields, `prev_hash`, `timestamp`, `agent_identity_hash`).
2. Implement append-only writer in the Rust runtime; expose append-only-only API to the rest of the runtime.
3. Implement `aegis verify <ledger-file>` walk-and-verify CLI.
4. Implement RFC 3161 TSA integration as an optional notarization step at session close.
5. Wire into F4, F5, F3 — every event from these features is itself a ledger entry, sharing the integrity guarantee.

## Related PRD Sections

- §4 F9 — Hash-Chained / Tamper-Evident Ledger
- §4 F4, F5, F3 — feed entries into the ledger

## Domain References

- RFC 6962 (Certificate Transparency)
- RFC 3161 (Time-Stamp Protocol)
- W3C Verifiable Credentials Data Model
- Git commit chain
