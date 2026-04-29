# 19. Explicit Write Grant Takes Precedence Over Broad Path Coverage

**Status:** Accepted
**Date:** 2026-04-29
**Domain:** Permission Manifest / F7 Time-Bounded Write Grants (extends ADR-009)

## Context

[ADR-009](009-read-only-default-with-explicit-write-grants.md) (F7) introduced
time-bounded `write_grants` so an operator can express "agent X may write
`/tmp/secret.txt` only between 10:00 and 11:00". After [issue #38](https://github.com/tosin2013/aegis-node/issues/38)
landed both engines (Rust `aegis-policy`, Go `pkg/manifest`) agreed on the
single-axis case: an expired grant filters out and the path falls back to
closed-by-default Deny. The `time-bounded-write.manifest.yaml` conformance
fixture deliberately avoided one second-order interaction by setting `tools: {}`,
and the comment in that file reserved the question for a follow-up — this ADR.

The interaction is:

```yaml
tools:
  filesystem:
    write: ["/tmp"]
write_grants:
  - resource: "/tmp/secret.txt"
    actions: ["write"]
    expires_at: "2026-04-29T10:30:00Z"
```

After `10:30:00Z`, the previous behavior was:

1. `find_write_grant("/tmp/secret.txt", write, now)` returns `None` because
   the only matching grant is time-expired and gets filtered out.
2. Code falls through to `tools.filesystem.write` and finds `/tmp` covers
   the path.
3. **Decision: Allow.**

That fall-through quietly weakens the time bound. A reader looking at the
manifest would reasonably read it as "this is the boundary on what I'll
allow for `/tmp/secret.txt`". They would not expect the broader `/tmp` rule
to reactivate the path after the explicit grant expired.

## Decision

**Explicit-takes-precedence.** If any `write_grant` names a resource for the
requested action, that grant's time window is decisive — broader
`tools.filesystem.write` coverage does NOT paper over an expired explicit
grant.

The classification has three outcomes:

| State | Trigger | Effect |
|---|---|---|
| **Valid** | At least one matching grant is in its time window | Allow / RequireApproval per the grant's `approval_required` flag and `approval_required_for: [any_write]` |
| **Expired** | At least one matching grant exists, but ALL are out of window | **Deny** + F9 Violation (per ADR-009's "expired grants emit violations" line) |
| **None** | No matching grant at all | Fall through to `tools.filesystem.write` (existing behavior) |

`Valid` wins over `Expired` — if two grants name the same resource and one
is in window, the in-window grant decides. This handles legitimate
overlap (e.g., a renewal grant added before the previous one expired).

The same rule applies to `check_filesystem_delete` for symmetry, even though
that path has no broader-rule fall-through to undermine. Sharing the
classifier keeps the two paths consistent and makes future actions
(`update`, `create`) inherit the rule for free.

## Consequences

**Positive**

- Time bounds become unforgeable from the manifest reader's perspective:
  if you write a time-bounded grant for a resource, that's the lifetime of
  access regardless of what else the manifest says.
- Aligns with ADR-009's stated intent that "expired grants emit violations".
- No schema change — `schemaVersion: "1"` stays frozen. Per the
  [Compatibility Charter](../COMPATIBILITY_CHARTER.md), this is a runtime
  semantics tightening that doesn't add or remove fields.

**Negative**

- A pre-#39 manifest that intentionally relied on fall-through (broad rule
  resumes after specific grant expires) will see different behavior under
  the new rule. We believe this pattern is rare in practice (the grants
  shipped in v0.5.0 / v0.8.0 fixtures don't use it), and a manifest author
  who genuinely wants "specific until X, then broad" can still express it
  by leaving the broad rule in place and removing the `expires_at` from the
  explicit grant.
- The classifier must scan all grants (rather than the previous early-return
  on first valid hit). The cost is bounded by the number of `write_grants`
  in the manifest — typically single digits — so the impact is negligible.

## Implementation

- **Rust** (`crates/policy/src/policy.rs`): new private `ExplicitGrant` enum
  (`Valid(&WriteGrant) | Expired | None`) and `Policy::classify_write_grant`
  helper. Both `check_filesystem_write` and `check_filesystem_delete`
  branch on the classifier instead of calling `find_write_grant` and falling
  through on `None`.
- **Go** (`pkg/manifest/decide.go`): mirror with `grantState` + `Manifest.classifyWriteGrant`.
- **Conformance**: a new fixture `tests/conformance/manifests/explicit-overrides-broad.manifest.yaml`
  pairs a broad `tools.filesystem.write: ["/tmp"]` rule with an explicit
  `write_grant` for `/tmp/secret.txt`. Cross-language cases added to
  `tests/conformance/cases.json`.

## Refs

- ADR-009 (F7 read-only default)
- Issue #38 / PR #39 — first half of F7 (parser + single-axis enforcement)
- Issue #40 — this ADR
- `time-bounded-write.manifest.yaml` — fixture comment that anticipated
  this question
