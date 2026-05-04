# Installing Aegis-Node

OS-specific install paths for getting `aegis` on your PATH and the
[examples](../examples/) running. Three approaches, in order of how
much host setup they need:

1. **[Docker](../examples/README.md#docker--fastest-no-host-toolchain-install)** —
   the published devbox image (`ghcr.io/tosin2013/aegis-node-devbox:latest`)
   has Rust + Go + oras + cosign + jq + node baked in. **Fastest path; OS-agnostic.**
   Covered in `examples/README.md` — start there if you don't have a strong
   reason to install natively.
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
    build-essential pkg-config \
    jq sqlite3 \
    nodejs npm pipx
pipx ensurepath
source ~/.bashrc
```

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
    jq sqlite \
    nodejs npm python3-pip
sudo dnf groupinstall -y "Development Tools"
python3 -m pip install --user pipx
python3 -m pipx ensurepath
source ~/.bashrc
```

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
mise install                                              # Rust 1.85, Go 1.23, cosign, node per mise.toml
source ~/.cargo/env                                       # if mise's rust used rustup, this puts cargo on PATH

# Build + install aegis
cargo install --locked --path crates/cli --features llama
aegis identity init --trust-domain aegis-node.local
```

**Two install gotchas you might hit:**

- *`cargo: command not found` after `mise install` succeeds.* mise
  installs tools but doesn't always put `cargo` on PATH on its own —
  on systems with a pre-existing `~/.rustup`, mise reuses rustup
  and cargo lands at `~/.cargo/bin/cargo`. Fix:
  `source ~/.cargo/env`. (Persist by adding it to `~/.bashrc`.)
- *`feature edition2024 is required` from `cargo install`.* Your Rust
  is too old (probably 1.83). Either re-run `mise install` from a
  checkout that pins Rust 1.85+, or upgrade rustup directly:
  `rustup install 1.85.0 && rustup default 1.85.0`.

### Step 2b: Native via system packages + rustup

For users who prefer not to adopt mise. Step 1 must already have
installed Go, cosign, and the build prereqs.

```bash
# Install rustup + Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source ~/.cargo/env
rustup install 1.85.0 && rustup default 1.85.0

# Clone + build + install aegis
git clone https://github.com/tosin2013/aegis-node.git
cd aegis-node
cargo install --locked --path crates/cli --features llama
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

Your Rust is older than 1.85. The locked dependencies in `Cargo.lock`
need Rust 1.85+. Two fixes:

- **From a Rust-1.85-pinned checkout** (post-v0.9.0 main, or this PR
  branch): re-run `mise install`.
- **Otherwise:** upgrade rustup directly —
  `rustup install 1.85.0 && rustup default 1.85.0`.

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
