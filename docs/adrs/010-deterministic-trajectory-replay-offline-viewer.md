# 10. Deterministic Trajectory Replay via Offline Single-File HTML Viewer

**Status:** Accepted
**Date:** 2026-04-27
**Domain:** Auditability / Forensics (F8)

## Context

The audit question "can we replay what happened?" is asked by every compliance officer and every incident responder. Existing approaches — log files in Splunk, screenshots, video screen recordings — are either incomplete, non-portable, or require live infrastructure to view. None of them survive the basic test of being shipped as evidence to an air-gapped auditor.

A regulated buyer (defense contractor, hospital, agency) needs replay capability that runs on a sealed laptop with no network access, with no dependency on the running agent or any external service.

## Decision

Aegis-Node ships a deterministic Trajectory Replay capability built around a single-file HTML viewer. Properties:

1. **Self-contained replay viewer.** A single static HTML file (CSS + JS embedded) that loads the local ledger file and renders a synchronized timeline of reasoning trajectory (F5) and access log (F4). No backend, no network calls, no CDN dependencies.
2. **Deterministic.** Replaying the same ledger always produces the same rendered output. No random ordering, no timezone-dependent rendering, no live data fetches.
3. **Self-sufficient ledger format.** The ledger contains all data necessary to reconstruct the session — no external lookups required. This shapes the F9 ledger schema.
4. **Air-gap shippable.** Auditors can be given a USB stick with `replay.html` + `ledger.jsonld` and reconstruct the session offline.
5. **Read-only viewer.** The viewer cannot mutate the ledger; integrity verification (F9 chain check) runs at load.

## Consequences

**Positive:**
- The replay artifact is the audit deliverable. Compliance reports become "here is the ledger and the viewer" rather than "let me schedule a Splunk query."
- Offline operation matches the air-gap default (Architecture Principle #3).
- Determinism enables reproducibility tests in CI: a known ledger fixture must render to a known DOM snapshot.

**Negative:**
- Single-file HTML constrains UI complexity — no large frontend frameworks, careful bundle-size discipline.
- Embedding everything inline requires a build step that produces valid, signed-or-hashed HTML; build pipeline must be maintained.
- Long sessions produce large ledgers; the viewer must handle them efficiently in browser memory.

## Domain Considerations

The "single static HTML you ship to auditors" pattern is borrowed from regulatory submission systems (FDA eCTD viewer, SEC EDGAR submissions). Auditors recognize it instantly as a credible evidence format.

## Implementation Plan

1. Define the offline replay HTML build pipeline (vanilla JS or compact framework, inlined assets, no `fetch()` or `XHR` against non-`file://` URLs).
2. Define the ledger schema such that no entry references resolve outside the ledger file.
3. Build the synchronized timeline UI: reasoning steps on one track, access events on another, approval events as overlays.
4. Add chain verification on load (uses the F9 hash chain); broken chain produces a prominent integrity warning.
5. Ship a CI test: a fixed ledger fixture renders to a fixed DOM snapshot.

## Related PRD Sections

- §4 F8 — Trajectory Replay
- §4 F9 — Hash-Chained Ledger
- §7 Architecture Principles (#3: Offline by default)

## Domain References

- FDA eCTD viewer pattern
- Local-first software principles
- Self-contained HTML offline applications
