---
version: alpha
name: Aegis-Node Community UI
description: >
  Design tokens for the Apache-2.0 Community WebUI shipped in v0.9.5 per
  ADR-031. Single source of truth — `ui/src/index.css` is regenerated from
  this file by the sub-phase 1b tooling (issue #163).

colors:
  bg:
    dark: "#0b0c0e"
    light: "#ffffff"
  bg-elev:
    dark: "#15171a"
    light: "#f5f6f8"
  fg:
    dark: "#edeef0"
    light: "#0b0c0e"
  muted:
    dark: "#9aa0a8"
    light: "#5b6573"
  border:
    dark: "#272a2d"
    light: "#d8dde5"
  border-strong:
    dark: "#363a3f"
    light: "#c0c6d0"
  accent:
    dark: "#7dd3fc"
    light: "#1e6fb8"
  success:
    dark: "#4ade80"
    light: "#15803d"
  warning:
    dark: "#facc15"
    light: "#a16207"
  danger:
    dark: "#f87171"
    light: "#b91c1c"
  focus-ring:
    dark: "#7dd3fc"
    light: "#1e6fb8"

typography:
  body:
    fontFamily: "{font.sans}"
    fontSize: "0.875rem"
    fontWeight: "400"
    lineHeight: "1.5"
    letterSpacing: "-0.005em"
  heading:
    fontFamily: "{font.sans}"
    fontSize: "1.25rem"
    fontWeight: "600"
    lineHeight: "1.25"
    letterSpacing: "-0.02em"
  caption:
    fontFamily: "{font.sans}"
    fontSize: "0.75rem"
    fontWeight: "500"
    lineHeight: "1.4"
    letterSpacing: "0"
  mono-id:
    fontFamily: "{font.mono}"
    fontSize: "0.8125rem"
    fontWeight: "500"
    lineHeight: "1.4"
    letterSpacing: "0"
  code:
    fontFamily: "{font.mono}"
    fontSize: "0.8125rem"
    fontWeight: "400"
    lineHeight: "1.5"
    letterSpacing: "0"

font:
  sans: >
    ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto,
    "Helvetica Neue", Arial, sans-serif
  mono: >
    ui-monospace, "SF Mono", "JetBrains Mono", "Fira Code", Menlo, Consolas,
    monospace

rounded:
  sm: "0.25rem"
  md: "0.5rem"
  lg: "0.75rem"
  full: "9999px"

spacing:
  xs: "0.25rem"
  sm: "0.5rem"
  md: "0.75rem"
  lg: "1rem"
  xl: "1.5rem"
  2xl: "2rem"

elevation:
  none:
    boxShadow: "none"
  raised:
    dark: "none"
    light: "0 1px 2px rgba(11, 12, 14, 0.06), 0 1px 1px rgba(11, 12, 14, 0.04)"
  floating:
    dark: "0 8px 24px rgba(0, 0, 0, 0.5), 0 0 0 1px {colors.border}"
    light: "0 8px 24px rgba(11, 12, 14, 0.12), 0 2px 6px rgba(11, 12, 14, 0.06)"

components:
  Button:
    typography: "{typography.body}"
    rounded: "{rounded.md}"
    padding: "{spacing.xs} {spacing.md}"
    variants:
      primary:
        backgroundColor: "{colors.fg}"
        textColor: "{colors.bg}"
        hover:
          opacity: "0.9"
        active:
          opacity: "0.82"
        disabled:
          backgroundColor: "{colors.muted}"
          textColor: "{colors.bg}"
          opacity: "0.5"
      secondary:
        backgroundColor: "{colors.bg-elev}"
        textColor: "{colors.fg}"
        border: "1px solid {colors.border}"
        hover:
          borderColor: "{colors.border-strong}"
          backgroundColor: "{colors.bg}"
        active:
          backgroundColor: "{colors.bg}"
        disabled:
          textColor: "{colors.muted}"
          opacity: "0.5"
      ghost:
        backgroundColor: "transparent"
        textColor: "{colors.muted}"
        hover:
          textColor: "{colors.fg}"
          backgroundColor: "{colors.bg-elev}"
        active:
          textColor: "{colors.fg}"
  Input:
    typography: "{typography.body}"
    rounded: "{rounded.md}"
    padding: "{spacing.xs} {spacing.md}"
    backgroundColor: "{colors.bg-elev}"
    textColor: "{colors.fg}"
    border: "1px solid {colors.border}"
    placeholderColor: "{colors.muted}"
    focus:
      borderColor: "{colors.border-strong}"
      boxShadow: "0 0 0 3px {colors.focus-ring}26"
    disabled:
      textColor: "{colors.muted}"
      opacity: "0.6"
  Select:
    typography: "{typography.body}"
    rounded: "{rounded.md}"
    padding: "{spacing.xs} {spacing.md}"
    backgroundColor: "{colors.bg-elev}"
    textColor: "{colors.fg}"
    border: "1px solid {colors.border}"
    focus:
      borderColor: "{colors.border-strong}"
      boxShadow: "0 0 0 3px {colors.focus-ring}26"
  Card:
    backgroundColor: "{colors.bg-elev}"
    textColor: "{colors.fg}"
    border: "1px solid {colors.border}"
    rounded: "{rounded.md}"
    padding: "{spacing.md}"
    elevation: "{elevation.none}"
  NavLink:
    typography: "{typography.body}"
    rounded: "{rounded.md}"
    padding: "{spacing.xs} {spacing.md}"
    backgroundColor: "transparent"
    textColor: "{colors.muted}"
    hover:
      textColor: "{colors.fg}"
      backgroundColor: "{colors.bg-elev}"
    active:
      backgroundColor: "{colors.bg-elev}"
      textColor: "{colors.fg}"
  IdentifierChip:
    typography: "{typography.mono-id}"
    rounded: "{rounded.sm}"
    padding: "1px {spacing.sm}"
    backgroundColor: "{colors.bg-elev}"
    textColor: "{colors.accent}"
    border: "1px solid {colors.border}"
  ToolCallChip:
    typography: "{typography.mono-id}"
    rounded: "{rounded.sm}"
    padding: "1px {spacing.sm}"
    backgroundColor: "{colors.bg-elev}"
    textColor: "{colors.accent}"
    border: "1px solid {colors.border}"
  ApprovalBanner:
    typography: "{typography.body}"
    rounded: "{rounded.md}"
    padding: "{spacing.md}"
    backgroundColor: "{colors.bg-elev}"
    textColor: "{colors.fg}"
    variants:
      advisory:
        borderLeft: "3px solid {colors.muted}"
      validating:
        borderLeft: "3px solid {colors.accent}"
      blocking:
        borderLeft: "3px solid {colors.warning}"
      escalating:
        borderLeft: "3px solid {colors.danger}"
---

# Aegis-Node Community UI — Design System

## Overview

This document is the canonical design source for the Apache-2.0 Community
WebUI shipped in v0.9.5 (per [ADR-031](adrs/031-community-webui-for-local-collaboration.md)). The
YAML front matter holds machine-readable tokens; the prose below explains the
intent behind each group so a reviewer can tell whether a proposed change
serves Aegis's positioning or fights it.

**One-sentence intent.** *Identifiers are first-class visual citizens.*
Workload IDs, OCI digests, manifest hashes, ledger entry IDs, SPIFFE URIs —
the strings that make Aegis's runtime claims verifiable — render in monospace
with the accent colour. They are content, not decoration. Anywhere the UI
shortens, hides, or visually demotes those strings to "make it look cleaner"
is a regression on the project's zero-trust positioning, and is rejected at
review time.

### Format

The YAML front matter follows [google-labs-code/DESIGN.md](https://github.com/google-labs-code/design.md)
with one extension: **colour tokens are objects keyed by `dark` and `light`**
rather than single values, so a single token reference like `{colors.accent}`
resolves to the correct shade for the active theme at CSS-generation time.
Components reference colours symbolically — they do not encode hex codes
directly — so the same component definitions work in both modes.

Token references use brace notation (`{colors.accent}`, `{spacing.md}`,
`{typography.body}`) and are resolved at CSS generation (issue #163), not at
runtime.

### Downstream consumers

| Consumer | What it reads | Issue |
|---|---|---|
| `ui/src/index.css` `@theme` block | `colors`, `font`, `rounded`, `spacing`, semantic colours | #163 |
| `ui/src/components/ui/` primitives (Button, Input, Select, Dialog, Tooltip, …) | `components.*` | #164 |
| Light-theme variant + WCAG audit | `colors.*.light` | #165 |

`ui/src/index.css` is the only file in `ui/` that this document directly
controls. Hand-edits to its `@theme` block are reverted by the regeneration
step — see the *Don'ts*.

## Colors

Seven core role colours plus three semantic colours, each defined in both
themes. The dark-mode palette is positioned on Radix `slateDark` steps so
every interactive state (hover, active, focus, border, solid) comes from
the same hue family — no opacity hacks, no ad-hoc tints.

- **`bg` / `bg-elev`** — page background and one level of elevated surface
  (cards, the nav bar's active pill, raised approval banners). The dark
  theme uses pure value differential between the two; the light theme adds
  a subtle shadow on `bg-elev` via the `elevation.raised` token so the
  boundary stays readable on white. Dark `bg` is `#0b0c0e` — cooler than
  pure black, biased very slightly toward Radix slate1.
- **`fg`** — primary text colour. Dark-mode is Radix `slate12`
  (`#edeef0`) — ~16:1 contrast on `bg` (AAA).
- **`muted`** — secondary text and inactive navigation. Dark-mode is Radix
  `slate10`-ish (`#9aa0a8`) — ~7:1 contrast on `bg` (AA normal, AAA large).
  The light-mode value is intentionally darker than the dark-mode value so
  contrast against `bg` stays balanced — dark text on white needs more
  weight than light text on near-black.
- **`border`** — `1px` rule used on inputs, selects, cards, and the top
  nav. Dark-mode is Radix `slate4` (`#272a2d`). Low contrast on purpose;
  the UI uses surface differential and 1px hairlines, not blurred shadows,
  as the structural signal.
- **`border-strong`** — used for focused-input borders and `Button.secondary`
  on hover. Dark-mode is Radix `slate6` (`#363a3f`). One step up from
  `border` so focus reads as a clear state change without a high-chroma
  accent ring at rest.
- **`accent`** — identifier colour. Used for workload IDs, OCI digests,
  manifest hashes, the shield logo, focus rings, links, approval-banner
  *validating* border. Dark-mode is `#7dd3fc` (Tailwind sky-300); light-mode
  steps down to a darker blue (`#1e6fb8`) that keeps AA contrast on white.
  **Identifier coverage is the most important property of this token** —
  see the *Do's*. Accent is **not** the default button-primary fill; per
  the modern dev-tool pattern (Linear, Vercel Geist), primary actions are
  inverted neutrals (`fg` on `bg`), and accent is reserved for inline
  identifier emphasis + focus state.
- **`success` / `warning` / `danger`** — semantic colours for approval
  state, quota warnings, and policy-block events. **Never the only
  signal** — they must pair with an icon or text label (see the *Don'ts*).
  The light-mode values step down to the `-700` band of each hue family so
  they remain AA against white text on a coloured chip; the dark-mode
  values are the Tailwind `-400` band tuned for legibility against
  `bg-elev`.
- **`focus-ring`** — `accent`, surfaced as a separate token so the
  WCAG-audit pass can override it without affecting visual identity.

## Typography

Five named scales: `body`, `heading`, `caption`, `mono-id`, `code`. Two
font stacks, defined under `font.*` and referenced symbolically. The
Community UI ships **without webfont downloads** — both stacks resolve to
system fonts exclusively so the localhost surface has no third-party
network dependency at runtime (consistent with ADR-031's loopback-only
stance and ADR-008's network-deny-by-default).

Tightness over bigness is the rule. System sans serifs (SF Pro Display on
macOS, Segoe UI Variable on Windows 11, Cantarell on GNOME) render heavier
than Inter/Geist at the same weight, so we cap body weight at 400 and
headings at 600, and apply small negative letter-spacing to mimic the
optical correction that webfonts apply at small sizes.

- **`body`** — the default. 0.875rem (14px) / 1.5 line-height / -0.005em
  letter-spacing. Tighter than the v0 placeholder (which was 15px/1.55/0);
  matches Linear/Geist chrome density.
- **`heading`** — page titles and section headings. Sans, semibold,
  -0.02em letter-spacing. Tight at large sizes — heading text should read
  as "compact precision," not "marketing hero."
- **`caption`** — small auxiliary text in chips and metadata strips. Sans,
  medium weight, 0.75rem (12px). **Never used for primary content.**
- **`mono-id`** — *the* identifier scale. Monospace, medium weight,
  0.8125rem (13px) — slightly smaller than body, so identifiers sit
  tightly inside `<IdentifierChip>`s without inflating their height. Used
  wherever the *Do's* call for identifier rendering.
- **`code`** — code blocks. Monospace, normal weight, 0.8125rem to pair
  with `mono-id`.

## Layout

Spacing scale is a 6-step `rem` ramp (`xs` 0.25 → `2xl` 2). All padding,
gap, and margin values across components reference the scale by name; no
ad-hoc dimensions in component definitions. Hand-edited Tailwind utilities
(`gap-1.5`, `py-3`, etc.) in `ui/src/` are tolerated during sub-phase 1d.2
but should be migrated to scale-anchored utilities once primitives land in
#164.

The Community UI is single-column on small viewports and uses a `max-w-3xl`
centered column on wider viewports (existing convention in `ui/src/components/TopNav.tsx`).
This document does not codify the container width — it's behavioural, not
token-shaped, and is the domain of individual surface layouts.

## Elevation & Depth

The design system is **flat by default**. Vercel Geist's pattern — replace
soft drop-shadows with 1px hairline borders at the `border` colour —
applies everywhere except true floating surfaces (popovers, dropdowns,
the future approval modal).

Two ramps: `raised` (cards, the active nav pill, message bubbles, banners)
and `floating` (Radix `<DialogContent>`, `<TooltipContent>`,
`<SelectContent>`).

- **Dark mode `raised`**: `boxShadow: "none"`. Elevation is encoded as a
  *surface differential* (`bg-elev` vs `bg`) plus the 1px `border` ring.
  Soft shadows read poorly on near-black backgrounds and undermine the
  precise, identifier-dense aesthetic.
- **Light mode `raised`**: subtle black-tinted shadow because the surface
  differential alone (`#ffffff` vs `#f5f6f8`) is too low-contrast on most
  monitors.
- **`floating` (both modes)**: a heavier shadow because tooltips and
  dropdowns must detach visually from the chrome they're anchored to.
  Dark mode pairs the shadow with a 1px `border` ring so the surface
  remains readable on near-black backgrounds.

Component definitions should reference `elevation.*` and let the generator
pick the right value per mode. Never hand-author `box-shadow` in component
files.

## Shapes

Three corner radii plus `full`:

- `sm` (4px) — chips, tool-call cards, inline tags. Small enough to read as
  "still rectangular," consistent with identifier-density positioning.
- `md` (8px) — buttons, inputs, selects, nav-link pills, cards. The default.
- `lg` (12px) — the chat input box, the approval banner, full-card
  containers.
- `full` — circular avatars and icon-only buttons (none currently in
  use; reserved).

No softer scale (`xl`, `2xl` rounded). The visual language is "precise tool,"
not "rounded SaaS dashboard."

## Components

Eight primitive components defined in the front matter. Each maps to one
of the existing or planned UI surfaces:

| Component | Used in | Notes |
|---|---|---|
| **`Button`** | Manifest Builder, chat send, model-library actions | Three variants: `primary` (inverted-neutral: `fg`-on-`bg`), `secondary` (bordered, surface), `ghost` (text-only with surface-fill on hover). Per the Linear/Geist pattern, primary buttons are *not* accent-filled — accent is reserved for identifier emphasis, focus rings, and links. |
| **`Input`** | Chat composer, Manifest Builder forms, model OCI ref entry | Always bordered. Focus state pulls border up to `border-strong` and adds a 3px translucent ring in `accent` — chroma is in the ring, not the border, so the input doesn't fight the surrounding chrome at rest. |
| **`Select`** | Backend picker, manifest tier selection, model dropdown | Same skin as `Input`; the chevron icon is `muted`, flipping to `fg` on hover. |
| **`Card`** | Home tiles, manifest preview, model library entries | `bg-elev` surface + 1px `border` + `md` padding (0.75rem). **No box-shadow.** The 1px border is the elevation signal in dark mode. |
| **`NavLink`** | Top nav (`ui/src/components/TopNav.tsx`) | Three states: default (`muted`), hover (`fg` + `bg-elev`), active (`bg-elev` + `fg`). Active state no longer uses `accent` for text — frees the accent treatment for genuine identifiers. |
| **`IdentifierChip`** | Workload IDs, OCI digests, manifest hashes, SPIFFE URIs, F9 ledger entry IDs | Monospace, accent text, `bg-elev` fill, 1px `border`, 4px radius, 1px-by-`sm` padding. Sits inline in body copy. Pairs with `<Tooltip>` to recover bytes when the chip is truncated. |
| **`ToolCallChip`** | Chat thread tool-call inline cards (per sub-phase 1d.2c) | Same visual treatment as `IdentifierChip` — a tool name *is* an identifier. The two components share token values; keeping them as separate entries documents intent. |
| **`ApprovalBanner`** | F3 approval cards in chat (per ADR-029) | Four tier variants — `advisory`, `validating`, `blocking`, `escalating` — differing **only** in left-border colour (3px). The tier label is also rendered in text inside the banner, never as colour alone. |

Variants under each component define hover, active, disabled, focus
states. The component spec is theme-agnostic — token references like
`{colors.accent}` resolve per mode at CSS generation.

## Do's and Don'ts

**Do**

- Render workload IDs, OCI digests, manifest hashes, SPIFFE URIs, and
  ledger entry IDs in `typography.mono-id` with `colors.accent`. The
  identifier is the trust artefact; the visual treatment makes that
  legible. (Example: the shield logo + "Aegis-Node" wordmark in
  `ui/src/components/TopNav.tsx` is `text-accent`.)
- Pair every semantic colour with an icon or text label. The `ApprovalBanner`
  tiers ship a left-border *and* a tier-name string for exactly this reason.
- Reference tokens by name when adding new UI (`text-accent`, `bg-bg-elev`,
  `rounded-md`) so the surface remains regenerable.
- Add new tokens here first, then regenerate `ui/src/index.css` via the
  sub-phase 1b tooling (issue #163).

**Don't**

- Use colour alone to convey approval/denial state on the F3 surface, or
  success/warning/danger anywhere else. Pair with an icon or text label.
  Users with colour-vision deficiency are a non-negotiable audience.
- Hand-edit `ui/src/index.css`'s `@theme` block. The file is generated from
  this document — hand-edits are reverted on the next regeneration.
- Introduce hex codes in component code. If a colour can't be expressed as
  a token reference, the design system is incomplete — add the token here
  first.
- Shorten or visually demote identifier strings to "tidy" a layout. Truncate
  at the *display* boundary with a tooltip showing the full string; never
  drop bytes from the rendered identifier without an explicit affordance to
  recover them.
- Add webfont `@import` statements or `<link rel="font">` tags. Both font
  stacks are system-only by design; introducing a webfont download breaks
  ADR-031's loopback-only stance.
- Use colour transparency (`opacity: 0.5`) to indicate disabled state on
  approval-related surfaces. Approval state is a security claim; "looks
  faded" is not a sufficient disabled signal. Use an explicit visual lock —
  disabled cursor, padlock icon, "blocked" text label.
