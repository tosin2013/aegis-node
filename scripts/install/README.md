# scripts/install/

One-shot installers for the Aegis-Node toolchain + CLI on common
Linux distros. Each script is the executable counterpart to the
corresponding section of [docs/INSTALL.md](../../docs/INSTALL.md) —
human prose for understanding, scripts for running.

| Script | Target | Validated against |
|---|---|---|
| [`ubuntu.sh`](ubuntu.sh) | Ubuntu 22.04 / 24.04 | `ubuntu:24.04` (Docker), 2026-05-04 |
| [`centos10.sh`](centos10.sh) | CentOS Stream 10 / Rocky 10 / Alma 10 / RHEL 10 | `quay.io/centos/centos:stream10`, 2026-05-04 |

## Usage

```bash
git clone https://github.com/tosin2013/aegis-node.git
cd aegis-node
bash scripts/install/ubuntu.sh        # or centos10.sh
```

What each script does:

1. **Install OS prereqs** (`apt-get` / `dnf`): build tools, clang/cmake, jq, sqlite, nodejs, etc.
2. **Install [`oras`](https://oras.land/)** v1.2.1 (no distro package; tarball install)
3. **Install [`mise`](https://mise.jdx.dev/)** if absent + activate in current shell + persist to `~/.bashrc`
4. **`mise trust` + `mise install`**: pin Rust 1.85.0, Go 1.23, cosign 2.4.1, node 20 per `mise.toml`
5. **`cargo build --release -p aegis-cli --features "llama litertlm"`** (workspace-aware → respects `Cargo.lock`)
6. **Install binary** to `~/.local/bin/aegis` + add to `PATH`
7. **`aegis identity init`** (idempotent — skips if CA already exists)
8. **Install MCP server extras**: `@modelcontextprotocol/server-filesystem` (Example 02), `uv`/`uvx` (Example 06)
9. **Smoke test**: run [Example 01](../../examples/01-hello-world/) end-to-end and verify the produced ledger

If step 9 prints `ledger ok: ... entries=5 ...` your install is good.

## Design notes

**Why `cargo build` + `cp` instead of `cargo install --path`?** The
workspace `Cargo.lock` lives at the repo root, not inside
`crates/cli/`. `cargo install --path crates/cli` doesn't pick up
the parent lockfile, regenerates dependencies fresh, and pulls
versions newer than what the project is tested against (e.g.
`time 0.3.47` requires Rust 1.88; the lockfile pins `time 0.3.41`
which works on Rust 1.85). `cargo build -p aegis-cli` from the
workspace root respects the lockfile directly.

**Why pin tool versions inside the script?** Reproducibility. The
mise installer (`https://mise.run | sh`) is the only "latest"
dependency — once mise is installed, the Rust/Go/cosign/node
versions come from the project's `mise.toml`, and `oras` is pinned
to v1.2.1 in the script itself. A re-run produces the same toolchain
state every time.

**Why install to `~/.local/bin` instead of `/usr/local/bin`?** No
sudo required for the binary install step. The system-level installs
(apt/dnf, oras, npm global) still need sudo, but the aegis binary
itself stays in user space.

**Idempotency.** Each script can be re-run safely:
- `apt-get install` / `dnf install` are idempotent.
- `mise install` reuses cached toolchain installs.
- `cargo build` reuses the existing target dir.
- `aegis identity init` is skipped if the CA already exists.
- `npm install -g` and `pipx install` are no-ops if the package is already global.

## Manual install path (for users who want to understand each step)

Read [`docs/INSTALL.md`](../../docs/INSTALL.md) — same flow, broken
out step-by-step with explanations. The scripts here are derived
from that doc and validated against it.

## Other distros / one-off paths

If you're on a distro not covered here (Fedora, Arch, openSUSE,
Debian-derivatives like Pop!_OS), either:

- Adapt one of these scripts. The Debian/Ubuntu script works on
  most apt-based distros; the CentOS script works on most dnf-based
  distros.
- Use the [Docker devbox image](../../examples/README.md#docker--fastest-no-host-toolchain-install)
  — `ghcr.io/tosin2013/aegis-node-devbox:latest` has the full
  toolchain pre-installed, OS-agnostic.

If you adapt a script for a new distro and it works cleanly,
contribute it back as `scripts/install/<distro>.sh`.
