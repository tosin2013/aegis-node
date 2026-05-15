# Release process

Operator-facing guide to cutting an Aegis-Node release. The workflow is
already implemented; this document explains the contract.

If you're cutting your first release on this project, read this top to
bottom once. After that, the *Cut flow* section is enough for the common
case.

## TL;DR — cutting a release

```bash
# 1. Confirm the milestone card exists in RELEASE_PLAN.md (Local
#    Milestones block) with the exact form `### vX.Y.Z — <name>`.
#    Em-dash, not hyphen. The release workflow fails fast if missing.
# 2. Push the tag.
git tag v0.9.5
git push --tags

# 3. Watch the `Release` workflow in GitHub Actions. It composes the
#    release body from the milestone card + the auto-generated
#    "What's Changed" list and creates a DRAFT GitHub Release.
# 4. Review the draft in the GitHub UI.
# 5. Click Publish.
```

Nothing else is automated. Drafts must be published manually — see
*Draft-then-publish policy* below for why.

## Cut flow

The workflow is `.github/workflows/release.yml`. It triggers on any
`v*.*.*` tag push and on `workflow_dispatch` for previews. The sequence:

1. **Tag is pushed** (or dispatched). The workflow resolves `VERSION`
   from either `GITHUB_REF_NAME` (tag push) or the dispatch input.
2. **Version validated** against `^v[0-9]+\.[0-9]+\.[0-9]+(-.+)?$`. A
   tag like `v0.9.5` or `v1.0.0-rc.1` passes; anything else fails the
   job in seconds.
3. **Pre-release flag detected** (see *Pre-release detection rules*).
4. **Milestone card extracted** from `RELEASE_PLAN.md` Local Milestones
   via [`scripts/release/extract-milestone-notes.sh`](../scripts/release/extract-milestone-notes.sh).
   Missing card → fail.
5. **GitHub milestone lookup** (report-only — see *Milestone-status
   block*).
6. **Auto-changelog generated.** If the tag exists on origin, the
   workflow calls GitHub's `releases/generate-notes` API (richer
   output: PR list, contributors). For dry-runs of un-pushed tags it
   falls back to `git log <prev-tag>..HEAD`.
7. **Release body composed.** Sections: *Scope* (milestone card),
   *Milestone status* (if found), *What's Changed* (auto-changelog).
8. **Draft GitHub Release created.** Operator opens it in the GitHub
   UI and clicks Publish.

## Hard contract: RELEASE_PLAN.md milestone card

Every tag must have a matching H3 card in the *Local Milestones* block
of [`RELEASE_PLAN.md`](../RELEASE_PLAN.md). The card format is:

```markdown
### v0.9.5 — Community UI
<!-- milestone-id: v0-9-5-community-ui -->
- **Status:** planned
- **Due:** 2026-10-20

<freeform paragraph describing the scope>
```

Three details that bite at 2 a.m.:

- **The separator is an em-dash (`—`, U+2014), not a hyphen.** The
  parser regex matches on the em-dash specifically. A hyphen card is
  invisible to the workflow.
- **The H3 line is the matching key.** The parser greps for
  `^### vX.Y.Z` (with optional em-dash + name). The `<!-- milestone-id
  -->` comment is informational only.
- **Card runs to the next H3 or the `<!-- /LOCAL MILESTONES -->`
  closing marker.** Whitespace between cards is fine; sub-headings
  (`####`) inside a card are fine. Another `### vX.Y.Z` ends the card.

Pre-release tags (`vX.Y.Z-rc.N`, `vX.Y.Z-beta.M`) fall back to the
base-version card. There is no separate card per RC.

If the parser can't find a card, the workflow's *Extract milestone
card* step prints:

```
::error::Missing RELEASE_PLAN.md milestone card for vX.Y.Z. Add an H3
entry to the LOCAL MILESTONES block before tagging.
```

Fix: add the card, push a new tag (or delete + retag), re-run.

## Pre-release detection rules

The workflow's `flag=` output decides whether the GitHub Release is
marked `--prerelease` or `--latest`:

| Tag pattern        | Flag           | Reason                                       |
|--------------------|----------------|----------------------------------------------|
| `v0.x.y`           | `--prerelease` | Pre-1.0 per `RELEASE_PLAN.md` versioning policy — community-preview, no compatibility guarantees. |
| `v1.0.0-rc.1` etc. | `--prerelease` | Suffixed semver (rc/beta/alpha/etc.).        |
| `v1.0.0` and later, no suffix | `--latest` | Stable semver ≥ 1.0.0.            |

The cutoff is hard-coded — there is no override. To ship a `v1.0.0+`
stable release as a pre-release (rare), tag it with a suffix like
`v1.0.1-postmortem.1` and let the detection do the right thing.

## Dry-run path

Use `workflow_dispatch` to preview a composed release body without
creating anything:

1. GitHub → Actions → *Release* → *Run workflow*.
2. `tag`: e.g. `v0.9.5`. The tag does **not** need to exist for a
   dry-run.
3. `dry_run`: `true` (default).
4. The job runs through *Resolve version → Detect pre-release →
   Extract milestone card → Look up milestone status → Generate
   auto-changelog → Compose release body* and prints the composed body
   in the *Compose release body* step's log. No draft is created.

When the tag hasn't been pushed yet, the auto-changelog falls back to
`git log` and prints a header noting the source. Pushed tags use
GitHub's generate-notes API and produce richer output (PR list +
contributors).

`dry_run: false` runs the full path and requires the tag to exist on
origin. If the tag is missing the *Create draft GitHub Release* step
fails fast with:

```
::error::Tag vX.Y.Z doesn't exist on origin. Push the tag (`git push
--tags`) before running with dry_run=false.
```

## Milestone-status block (report-only)

If a GitHub milestone matches the tag (`vX.Y.Z` prefix), the workflow
renders an open/closed issue summary into the release body:

```markdown
## Milestone status

[**v0.9.5 — Community UI**](…) — open: `3` · closed: `12`

_⚠️ 3 issue(s) still open against this milestone — see linked
milestone for the in-flight scope._
```

**Open issues do not block the release.** This is the *Balanced
policy*: tags are operator-controlled and intentional; the milestone
count is informational. If you really want a clean milestone before
shipping, close the issues first.

If no milestone matches the tag the section is omitted entirely (and a
single `::warning::` annotation lands in the workflow log).

## Draft-then-publish policy

The workflow always creates a **draft** GitHub Release. The operator
must click Publish in the GitHub UI for the release to become visible.

This is intentional pre-1.0 caution:

- Catches typo tags (`v0.9.5` vs `v0.95`) before they reach users.
- Catches scope mismatches (the milestone card says one thing, the
  auto-changelog shows another).
- Gives one last chance to edit the body in the GitHub UI before it
  goes out.

Once a release is published, the tag, the body, and the asset list all
become immutable in any way that matters — drafts are the only point
of safe editing.

There is no plan to change this before v1.0.0. After v1.0.0, if a
team decides auto-publish is acceptable, that change goes through an
ADR.

## Hotfix policy

Tag from `main`. There are no release branches today.

If `main` has drifted past the bug being patched, cherry-pick the fix
onto a hotfix branch, fast-forward `main`, tag from there. No long-
lived release branches; the project's distribution surface is a single
binary, not a multi-branch product.

## Common failures and recovery

| Symptom | Cause | Fix |
|---|---|---|
| `Missing RELEASE_PLAN.md milestone card for vX.Y.Z` | No matching `### vX.Y.Z — <name>` H3 in Local Milestones, or hyphen used instead of em-dash. | Add the card, then either push a new tag or `git tag -d vX.Y.Z && git tag vX.Y.Z <sha> && git push --tags --force` (only for unpublished tags). |
| `Version 'X' doesn't match vX.Y.Z[-suffix] pattern` | Tag is something like `0.9.5` (missing `v`), `v0.9` (missing patch), `v0.9.5_rc1` (underscore not hyphen). | Delete + retag with the canonical form. |
| `Tag vX.Y.Z doesn't exist on origin` during a non-dry-run dispatch | You ran `workflow_dispatch` with `dry_run=false` but the tag is still local. | `git push --tags`, then re-run the dispatch. |
| Auto-changelog reads "Generated from `git log` …" instead of the rich format | The tag is not yet pushed to origin. | Expected for dry-runs. The pushed-tag run gets the richer GitHub-API output. |
| Workflow runs but no release appears | The release was created as a draft; drafts only show on the *Releases* page when the maintainer is signed in. | Open the *Releases* page while signed in; click Publish on the draft. |
| Two workflow runs racing (rare) | Multiple tags pushed in quick succession. | The `concurrency` group is keyed on `github.ref_name`; same-tag runs serialize. Different tags run in parallel, which is fine. |

## References

- [`.github/workflows/release.yml`](../.github/workflows/release.yml) — the workflow.
- [`scripts/release/extract-milestone-notes.sh`](../scripts/release/extract-milestone-notes.sh) — milestone-card parser.
- [`RELEASE_PLAN.md`](../RELEASE_PLAN.md) — milestone card source of truth; versioning policy at the top.
- [`CONTRIBUTING.md`](../CONTRIBUTING.md) — repo-wide contribution policy.
