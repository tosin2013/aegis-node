# Installing Aegis-Node

OS-specific install paths for getting `aegis` on your PATH and the
[examples](../examples/) running.

## Quickest: run the installer script

Empirically validated on `ubuntu:24.04` and `quay.io/centos/centos:stream10`
(2026-05-04). One command per OS, runs Steps 1–4 below end-to-end and
finishes by running Example 01 as a smoke test:

```bash
git clone https://github.com/tosin2013/aegis-node.git
cd aegis-node

# Ubuntu 22.04 / 24.04
bash scripts/install/ubuntu.sh

# CentOS Stream 10 / Rocky 10 / Alma 10 / RHEL 10
bash scripts/install/centos10.sh
```

If you'd rather understand every step or your distro isn't covered,
keep reading — the rest of this doc is the manual prose form of the
same flow. See also [`scripts/install/README.md`](../scripts/install/README.md).

## Other install paths

1. **[Docker](../examples/README.md#docker--fastest-no-host-toolchain-install)** —
   the published devbox image (`ghcr.io/tosin2013/aegis-node-devbox:latest`)
   has Rust + Go + oras + cosign + jq + node baked in. OS-agnostic.
   Covered in `examples/README.md`.
2. **[Native via mise](#step-2a-native-via-mise)** — toolchain version manager;
   pins everything per `mise.toml`. Same on Ubuntu and CentOS once mise itself is installed.
3. **[Native via system packages + rustup](#step-2b-native-via-system-packages--rustup)** —
   for users who prefer not to adopt mise.

The native paths follow a Step 1 → 2 → 3 → 4 flow:

- **Step 1**: install OS prerequisites (your distro's package manager)
- **Step 2**: install Aegis (pick path 2a or 2b)
- **Step 3**: per-example extras (npm + pipx)
- **Step 4**: verify with Example 01

---

## Step 1: OS prerequisites

Pick your distro. The packages here are everything you need for the
toolchain installs and for running every example.

### Ubuntu (24.04 / 22.04)

```bash
sudo apt-get update
sudo apt-get install -y \
    curl ca-certificates git \
    build-essential pkg-config clang cmake unzip \
    jq sqlite3 \
    nodejs npm pipx
pipx ensurepath
source ~/.bashrc
```

`clang` provides `libclang.so` (needed by `bindgen`, transitively
required by `llama-cpp-sys-2`); `cmake` builds llama.cpp from source.
Skipping these gets you `Unable to find libclang` partway through
`cargo build`.

**Install `oras`** (no apt package):

```bash
curl -fsSL "https://github.com/oras-project/oras/releases/download/v1.2.1/oras_1.2.1_linux_amd64.tar.gz" \
    | sudo tar -xz -C /usr/local/bin oras
```

If you're using **Path 2b (rustup, no mise)**, also install Go (the
apt `golang-go` package may be too old on 22.04):

```bash
curl -fsSL "https://go.dev/dl/go1.23.4.linux-amd64.tar.gz" \
    | sudo tar -xz -C /usr/local
export PATH=/usr/local/go/bin:$PATH
echo 'export PATH=/usr/local/go/bin:$PATH' >> ~/.bashrc
```

### CentOS 10 (Stream / Rocky / Alma — RHEL family)

```bash
sudo dnf install -y \
    curl ca-certificates git \
    gcc gcc-c++ make pkgconf-pkg-config \
    clang cmake unzip \
    jq sqlite \
    nodejs npm python3-pip
sudo dnf groupinstall -y "Development Tools"
python3 -m pip install --user pipx
python3 -m pipx ensurepath
source ~/.bashrc
```

`clang` provides `libclang.so` (needed by `bindgen` for
`llama-cpp-sys-2`); `cmake` builds llama.cpp from source. Skipping
these gets you `Unable to find libclang` partway through `cargo build`.

All packages above are in CentOS 10's base / AppStream repos —
**you should not need EPEL** for the examples. If a specific package
goes missing on your build of CentOS 10 / Rocky 10 / Alma 10, install
EPEL via URL since the `epel-release` package isn't in the default
extras repo on EL10:

```bash
sudo dnf install -y \
    https://dl.fedoraproject.org/pub/epel/epel-release-latest-10.noarch.rpm
sudo dnf config-manager --set-enabled crb       # CodeReady Builder, prereq for EPEL
```

**Install `oras`** (no dnf package):

```bash
curl -fsSL "https://github.com/oras-project/oras/releases/download/v1.2.1/oras_1.2.1_linux_amd64.tar.gz" \
    | sudo tar -xz -C /usr/local/bin oras
```

If you're using **Path 2b (rustup, no mise)**, also install Go and
cosign:

```bash
# Go
curl -fsSL "https://go.dev/dl/go1.23.4.linux-amd64.tar.gz" \
    | sudo tar -xz -C /usr/local
export PATH=/usr/local/go/bin:$PATH
echo 'export PATH=/usr/local/go/bin:$PATH' >> ~/.bashrc

# cosign
curl -fsSL -o /tmp/cosign \
    "https://github.com/sigstore/cosign/releases/download/v2.4.1/cosign-linux-amd64"
sudo install -m 0755 /tmp/cosign /usr/local/bin/cosign
```

---

## Step 2: install Aegis

Pick **2a (mise)** for pinned versions matching CI, or **2b (rustup +
distro Go)** for a simpler dependency tree.

### Step 2a: Native via mise

Same commands on every Linux once Step 1 is done.

```bash
# Install mise itself (one-time)
curl https://mise.run | sh
eval "$(~/.local/bin/mise activate bash)"                 # current shell
echo 'eval "$(~/.local/bin/mise activate bash)"' >> ~/.bashrc   # persist

# Clone + install pinned toolchain
git clone https://github.com/tosin2013/aegis-node.git
cd aegis-node
mise trust mise.toml                                      # required before mise install for new checkouts
mise install                                              # Rust 1.85, Go 1.23, cosign, node per mise.toml
source ~/.cargo/env                                       # if mise's rust used rustup, this puts cargo on PATH

# Verify Rust is actually 1.85+. mise can defer to a pre-existing
# rustup whose RUSTUP_TOOLCHAIN env var pins an older toolchain —
# in that case rustc --version will show <1.85 even though mise
# install said it succeeded.
rustc --version
# If <1.85.0:
#   rustup install 1.85.0 && rustup default 1.85.0
#   unset RUSTUP_TOOLCHAIN     # mise may set this; clears the override

# Build aegis from the workspace root (uses the workspace Cargo.lock,
# which pins dependencies to Rust-1.85-compatible versions).
cargo build --release -p aegis-cli --features llama

# Install the binary to ~/.local/bin (no sudo needed)
mkdir -p ~/.local/bin
cp target/release/aegis ~/.local/bin/aegis
export PATH="$HOME/.local/bin:$PATH"
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc

# Bootstrap the local CA
aegis --version
aegis identity init --trust-domain aegis-node.local
```

**Why `cargo build + cp` instead of `cargo install --path`:**
`cargo install --path crates/cli` is the idiomatic single-command
install, but it ignores the workspace `Cargo.lock` (the workspace
root's lockfile isn't auto-picked-up for path installs of workspace
members). It then regenerates dependencies fresh, which may pull
versions newer than what the project tests against — for Aegis-Node
that means `time 0.3.47` (requires Rust 1.88) and others. `cargo
build -p` from the workspace root respects `Cargo.lock` directly.

**Three install gotchas you might hit:**

- *`cargo: command not found` after `mise install` succeeds.* On
  systems with a pre-existing `~/.rustup`, mise reuses rustup and
  cargo lands at `~/.cargo/bin/cargo` (not via mise's shims). Fix:
  `source ~/.cargo/env`. Persist by adding to `~/.bashrc`.
- *`mise install` ran, `rustup default 1.85.0` ran, but `rustc
  --version` still shows 1.83.* mise is setting `RUSTUP_TOOLCHAIN=1.83.0`
  from the project's `mise.toml`, and that env var overrides rustup's
  default. Two fixes: (a) `unset RUSTUP_TOOLCHAIN` in current shell,
  or (b) check out a branch whose `mise.toml` pins 1.85+ (the
  v0.9.0 release does).
- *`feature edition2024 is required` from `cargo install`.* You're
  using `cargo install --path` — switch to the `cargo build + cp`
  flow above. The workspace `Cargo.lock` pins Rust-1.85-compatible
  versions of clap and indexmap; `cargo install --path` regenerates
  the lockfile fresh and pulls newer versions that need Rust 1.88.

### Step 2b: Native via system packages + rustup

For users who prefer not to adopt mise. Step 1 must already have
installed Go, cosign, and the build prereqs.

```bash
# Install rustup + Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source ~/.cargo/env
rustup install 1.85.0 && rustup default 1.85.0
rustc --version                                           # confirm 1.85.0+

# Clone + build aegis
git clone https://github.com/tosin2013/aegis-node.git
cd aegis-node
cargo build --release -p aegis-cli --features llama

# Install binary to ~/.local/bin
mkdir -p ~/.local/bin
cp target/release/aegis ~/.local/bin/aegis
export PATH="$HOME/.local/bin:$PATH"
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc

aegis --version
aegis identity init --trust-domain aegis-node.local
```

---

## Step 3: per-example extras

Same on every OS. After Step 2 finishes:

```bash
# Example 02 default mode (filesystem MCP)
npm install -g @modelcontextprotocol/server-filesystem

# Example 06 (SQLite MCP via uvx)
pipx install uv
```

For Example 02 extended mode (live web research), set
`FIRECRAWL_API_KEY=fc-...` in your environment when running the
example. No additional install needed — the firecrawl-mcp server
loads via `npx`.

---

## Step 4: verify the install

From inside your `aegis-node` checkout, run Example 01 end-to-end:

```bash
cd examples/01-hello-world
bash setup.sh
cd /tmp/aegis-example-01
aegis run --manifest manifest.yaml --model model.gguf \
    --workload hello-world --instance inst-001 \
    --prompt "$(cat prompt.txt)"
cat output/greeting.txt           # should print "hello, world from aegis-node"
aegis verify ledger-*.jsonl       # should print "ledger ok: ... entries=5 ..."
```

If both succeed, your install is good. Continue through `examples/02-mcp-research-assistant/`
… `06-mcp-finance-sqlite/` (each example's README has its own "Run it"
block).

---

## Troubleshooting

### `cargo: command not found` after `mise install`

On RHEL/CentOS systems with a pre-existing `~/.rustup`, mise reuses
rustup and cargo lands at `~/.cargo/bin/cargo` (not via mise's shims).
Fix: `source ~/.cargo/env`, persist by adding to `~/.bashrc`.

### `feature edition2024 is required` from cargo

Two possible causes:

1. **You're on Rust < 1.85.** The workspace `Cargo.lock` pins clap 4.6
   and indexmap 2.14, which require Rust 1.85+. Fix:
   ```bash
   source ~/.cargo/env
   rustup install 1.85.0 && rustup default 1.85.0
   unset RUSTUP_TOOLCHAIN     # if mise was setting it to an older version
   rustc --version            # confirm 1.85.0+
   ```

2. **You're using `cargo install --path crates/cli` instead of
   `cargo build`.** The path install ignores the workspace
   `Cargo.lock` and regenerates dependencies fresh, pulling
   `time 0.3.47` (Rust 1.88), `wasip2 1.0` (Rust 1.87), etc. — all
   newer than the lockfile would pick. Fix: switch to the
   `cargo build + cp` flow described in [Step 2a](#step-2a-native-via-mise).

### `aegis pull` fails with cosign verification error

The `ghcr.io` registry + the publish workflow's keyless OIDC chain
are required for verification. If you're behind a strict proxy or
running offline, mirror the models per
[docs/MODEL_MIRRORING.md](MODEL_MIRRORING.md).

### `mcp-server-filesystem: command not found` (Example 02)

`npm install -g @modelcontextprotocol/server-filesystem` requires
that `npm`'s global prefix is on PATH. Default location is
`/usr/local/lib/node_modules/.bin`; adjust if your npm is configured
differently.

### `uvx: command not found` (Example 06)

`pipx install uv` puts `uv` (and `uvx`) under `~/.local/bin/`.
After install, run `pipx ensurepath` and re-source your shell rc.

### `aegis identity init` says "CA already initialized"

Harmless — the local CA persists in `~/.config/aegis/identity/`.
If you've run `aegis identity init` before on this machine, it's
done. Skip the step.
