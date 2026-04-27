# 6. Structured Access Log in JSON-LD for SIEM Integration

**Status:** Accepted
**Date:** 2026-04-27
**Domain:** Auditability / Observability (F4)

## Context

The compliance question "what data did the agent touch?" is the most frequent and most tedious question in any post-incident review. A free-form text log cannot answer it precisely. Existing AI agent runtimes log either nothing useful (just LLM prompts and responses) or a firehose of unstructured tool-call traces that no SIEM can parse without custom adapters.

Compliance frameworks (CMMC, HIPAA, FedRAMP) require log evidence with specific fields, in machine-parseable form, ingestible by the security team's existing SIEM (Splunk, Sentinel, Elastic, Chronicle, etc.).

## Decision

Every file read, file write, API call, database query, and resource access produces a structured Access Log entry. Properties:

1. **Mandatory fields** in every entry: agent identity (F1), resource URI, access type, bytes accessed, nanosecond-precision timestamp, session ID, and the F5 reasoning-step ID that triggered the access.
2. **JSON-LD format** for export — semantically annotated for SIEM ingestion and for cross-organization correlation when multiple agents interact.
3. **Atomic writes.** A partial log entry is a critical violation; the runtime must guarantee either a complete entry or none.
4. **Cross-referenced.** Access log entries and reasoning trajectory entries (F5) are stored separately but linked by session ID and reasoning-step ID — so an auditor can answer "what data did this reasoning step touch?" in O(1).
5. **Stored inside the F9 hash chain.** Access entries are themselves ledger entries — they participate in the tamper-evident chain and cannot be selectively edited.

## Consequences

**Positive:**
- Direct answer to the compliance question; exportable as evidence to auditors.
- SIEM-native integration on day one (no custom parsers).
- Cross-reference with reasoning trajectory (F5) gives explainability rather than just access lists.

**Negative:**
- High log volume for I/O-heavy agents — must support log rotation and streaming-export.
- Mandatory metadata (e.g., bytes accessed) must be captured at every I/O boundary; instrumentation cost is non-trivial.
- JSON-LD context vocabulary must be defined and versioned; consumers depend on it.

## Domain Considerations

JSON-LD is W3C-standard and is the same format used in F9 for ledger entries, keeping a single ingestion path for SIEMs. Field selection is informed by NIST 800-53 AU-2/AU-3 audit content requirements.

## Implementation Plan

1. Define the Access Log JSON-LD `@context` document with stable URIs.
2. Wire the Rust runtime to emit structured events at every I/O syscall the manifest covers (open, read, write, connect, exec).
3. Implement a streaming exporter (file → stdout / file → syslog → SIEM).
4. Ship reference dashboards for Splunk and Elastic to demonstrate ingestion.
5. Conformance test: a multi-tool agent run produces an Access Log such that every tool result correlates exactly to one access entry.

## Related PRD Sections

- §4 F4 — File / API / Resource Access Log
- §4 F5 — Reason + Action Trajectory
- §4 F9 — Hash-Chained Ledger

## Domain References

- JSON-LD 1.1 W3C Recommendation
- NIST 800-53 AU-2, AU-3 (Audit Events, Content)
- HIPAA §164.312(b) (audit controls)
