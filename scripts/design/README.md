# scripts/design/

Tooling that derives downstream artefacts from [`docs/DESIGN.md`](../../docs/DESIGN.md).

## `render-tokens.mjs`

Generates `ui/src/index.css`'s `@theme` block from the YAML front matter in
`docs/DESIGN.md`. `docs/DESIGN.md` is the single source of truth for design
tokens; this script writes the derived CSS so the two artefacts can't drift.

Run from `ui/`:

```bash
pnpm design:tokens
```

The script is also wired as a `predev` / `prebuild` hook in `ui/package.json`,
so the tokens regenerate before every Vite build (including the
`crates/ui-server` build-script invocation that embeds the SPA into the
`aegis` binary).

### What it emits

| Front-matter key | CSS variable               | Notes                                       |
|------------------|----------------------------|---------------------------------------------|
| `colors.<name>`  | `--color-<name>`           | Dark-mode values only at this stage         |
| `font.<name>`    | `--font-<name>`            | Whitespace collapsed                        |
| `typography.<name>.fontSize` | `--text-<name>` | Font-size only (richer typography in #164)  |
| `rounded.<name>` | `--radius-<name>`          | Tailwind v4 naming convention               |
| `spacing.<name>` | `--spacing-<name>`         |                                             |

### What it does **not** emit (per issue #163)

- `components.*` — primitive components consume these directly in #164.
- `colors.*.light` — the light-theme variant lands in #165.
- `elevation.*` — same scope deferral as components.

### Idempotency

Running the script twice on a clean repo produces no diff. CI can use this
to catch drift:

```bash
pnpm design:tokens
git diff --exit-code ui/src/index.css
```

This CI check is filed as a follow-up in #163 ("can be a follow-up") rather
than landed here.
