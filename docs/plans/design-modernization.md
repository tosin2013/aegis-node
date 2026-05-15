# Design modernization — refined dev-tool pass

**Status:** proposal · authored 2026-05-15
**Anchor issue:** follow-up on #164 (primitives) — the design system landed
backwards-compatible by design so the regenerated CSS produced byte-identical
visible output. That kept #162/#163/#164 reviewable. This document is the
follow-up that actually modernizes the visual treatment.

## Goal

Move the Community UI from "placeholder dark theme" to a refined dev-tool
aesthetic in the Linear / Vercel Geist / Radix Themes family — without
breaking the project's "zero-trust, identifier-dense" positioning per
ADR-031.

## What "modern dev tool" means here

Synthesised from four references (full notes in commit history):

- **Vercel Geist** — shadows are *replaced* by 1px hairline borders
  (`0 0 0 1px <border>`). Flat elevation. 14px body. Bright accent
  (`#3291ff` link blue) used sparingly.
- **Radix `slateDark`** — 12-step neutral scale where every interactive
  state (hover, active, focus, border, solid, solid-hover) comes from the
  *same hue family*. No opacity hacks. Steps used: 1 (bg), 2/3 (elev), 4
  (border), 10 (muted text), 12 (fg).
- **Linear** — identifiers ("ENG-1234", PR refs) render as monospace chips
  with a subtle filled bg + 1px ring, not as colored text. Primary actions
  are colored; links stay neutral underlined. Border-less surfaces use
  background steps and whitespace for structure.
- **Resend docs** — inline code never raw; always a monospace chip with
  filled background. Mirrors Linear's identifier treatment.

## Constraints to respect

| Constraint | Source | Implication |
|---|---|---|
| System fonts only — no webfont downloads | ADR-031 (loopback-only stance) | Can't ship Geist Sans / Inter literally. Use system stack at sizes/weights that pair well with SF Pro / Segoe UI Variable |
| Identifiers are first-class visual citizens | ADR-031 + `docs/DESIGN.md` *Don'ts* | Identifier-chip pattern is non-negotiable; the modernization should *strengthen* it, not weaken it |
| WCAG AA contrast for fg + accent on bg | Sub-issue #165 | All proposed tokens must hit AA at minimum; AAA where free |
| Dark-first; light theme deferred | Sub-issue #165 | Dark palette only in this pass |

## Token deltas (dark mode)

Current values are from the placeholder palette preserved through #162/#163.
Proposed values come from the research, rounded onto Radix slate where the
research recommended slate steps.

| Token       | Current     | Proposed     | Delta + rationale |
|-------------|-------------|--------------|-------------------|
| `bg`        | `#0b0d10`   | `#0b0c0e`    | Slightly cooler — drop a faint green hint that read "early-2000s terminal" |
| `bg-elev`   | `#11151a`   | `#15171a`    | One step lighter — pairs with border-only elevation pattern |
| `fg`        | `#e5e9ef`   | `#edeef0`    | Radix `slate12`. Bumps contrast against `bg` from ~14.6:1 to ~16.2:1 (AAA) |
| `muted`     | `#8a93a3`   | `#9aa0a8`    | Brighter — current was muddy at AA boundary; new is solid AA on `bg-elev` chips |
| `border`    | `#1f2630`   | `#272a2d`    | More visible — current borders disappeared on Linux LCDs at normal viewing distance |
| `accent`    | `#7cc4ff`   | `#7dd3fc`    | Indistinguishable hex-wise (~1 chroma step) — but lock onto the Tailwind `sky-300` value so engineers reading code map it instantly |
| `border-strong` | _new_   | `#363a3f`    | Radix `slate6`. For focused-input rings + button-secondary borders that need to read present |

### Body typography

| Property | Current | Proposed | Rationale |
|---|---|---|---|
| body fontSize | `0.9375rem` (15px) | `0.875rem` (14px) | Linear / Geist chrome density |
| body lineHeight | `1.55` | `1.5` | Tighter; reads as "tool," not "marketing" |
| body letterSpacing | `0` | `-0.005em` | Mimics the optical correction Geist / Inter apply at small sizes |
| heading fontWeight | `600` | `600` *(unchanged)* | System sans at 700 reads too heavy vs Inter/Geist; 600 is correct |
| heading letterSpacing | `-0.01em` | `-0.02em` | Tighter headings; closer to Geist heading optics |

### Elevation

**No box-shadows.** Replace with 1px borders at `border` (`slate4`). Geist's
pattern: `box-shadow: 0 0 0 1px var(--color-border)` for raised surfaces,
which is equivalent to a 1px ring but stacks cleanly with hover states. For
*floating* surfaces (popovers, dropdowns) keep a single low-spread shadow
because tooltips/dropdowns need to detach visually from the underlying
chrome — but use it sparingly.

### Identifier-chip pattern (the headline change)

Today every workload-ID, OCI digest, manifest hash is rendered as colored
monospace inline text. Move to a *chip* treatment per Linear / Resend:

```
<span class="font-mono text-xs text-accent bg-[var(--color-bg-elev)]
             border border-[var(--color-border)] rounded
             px-1.5 py-0.5">
  sha256:7c4a3b…
</span>
```

This is small enough to sit in body copy, distinct enough to read as
"identifier — important," and consistent across the Chat verifiable badge,
tool-call card name, model-library OCI digest, and (later) the manifest
builder's workload-ID field.

Codify as a new component in `docs/DESIGN.md`: `IdentifierChip`.

### Button primary

Current: `bg-accent text-bg` (accent fill). Modern dev-tool pattern (Linear,
Geist, Vercel): primary buttons are **inverted neutrals** — `bg-fg
text-bg` — and accent is reserved for *links*, *focused state*, *important
inline info*. This frees `accent` to do its identifier-emphasis job without
visually competing with every CTA.

For Aegis specifically: chat's *Send* button is more of a "secondary"
action than a hero CTA, so it stays in the `secondary` variant either way.
But `default` (the primary variant) should be re-spec'd as inverted neutral
so it lands correctly the first time someone uses it.

## Component-level changes (motivated by tokens, but spec'd here)

- **Button** — `default` variant flips from accent-fill to fg-on-bg. Heights
  drop one rung: `sm` 7 → 7, `md` 9 → 8, `lg` 11 → 9. Tighter chrome.
- **Input / Textarea / Select** — focus ring drops from `accent/30` to
  `accent/40` and the border-on-focus moves from `accent` to `border-strong`
  for less chroma at rest.
- **Card** — `padding` drops from `spacing.lg` (1rem) to `spacing.md` (0.75rem).
  Border is `1px solid border` and the surface is `bg-elev`. **No shadow.**
- **IdentifierChip** — *new* primitive. Renders monospace, accent-colored
  text on `bg-elev` with `border` 1px and `radius.sm`. Has a `truncate` prop
  that pairs with a tooltip on hover.

## Out of scope (deferred to other issues)

- Light-theme variant — sub-issue #165.
- WCAG audit — sub-issue #165.
- Component primitives for non-Chat surfaces (Manifest, Models, Home) — same
  follow-up as #164's deferred migration list.
- Motion / micro-interactions beyond the existing 150ms transition-colors —
  separate pass once we have a Storybook to iterate in.

## Plan of attack for this PR

1. Update `docs/DESIGN.md` front matter with the new token values + the
   `IdentifierChip` component spec. Update prose sections that reference
   specific values.
2. Run `pnpm design:tokens`. Verify the regenerated `@theme` block.
3. Update primitives in `ui/src/components/ui/`:
   - `button.tsx` — flip `default` variant, drop sizes one rung.
   - `input.tsx`, `textarea.tsx`, `select.tsx` — refine focus rings.
   - `card.tsx` — tighten padding, drop shadow.
   - *new* `identifier-chip.tsx`.
4. Restyle Chat (`ui/src/pages/Chat.tsx`):
   - Header: tighter typography, smaller icon.
   - Message bubbles: refine padding, use new colors, identifier chip for
     verifiable-anchor.
   - Tool-call cards: replace inline `font-mono text-xs text-accent` with
     `<IdentifierChip>` for the tool name. Tighten card padding.
   - Composer: use updated Button + Textarea (smaller heights, refined
     focus rings).
5. Verify `pnpm build`, screenshot via Vite dev server (loopback only).
6. PR description includes before/after screenshots so the reviewer can
   approve the visual direction.
