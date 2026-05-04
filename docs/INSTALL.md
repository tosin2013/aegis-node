# Installing Aegis-Node

OS-specific install paths for getting `aegis` on your PATH and the
[examples](../examples/) running. Three approaches, in order of how
much host setup they need:

1. **[Docker](../examples/README.md#docker--fastest-no-host-toolchain-install)** —
   the published devbox image (`ghcr.io/tosin2013/aegis-node-devbox:latest`)
   has Rust + Go + oras + cosign + jq + node baked in. **Fastest path; OS-agnostic.**
   Covered in `examples/README.md` — start there if you don't have a strong
   reason to install natively.
2. **[Native via mise](#native-via-mise)** — toolchain version manager;
   pins everything per `mise.toml`. Same on Ubuntu and CentOS once mise itself is installed.
3. **[Native via system packages + rustup](#native-via-system-packages--rustup)** —
   for users who prefer not to adopt mise. Per-OS instructions below.

## Native via mise

Same on every Linux. Install OS prereqs first (see per-OS section below
for `apt-get` / `dnf` commands), then:

```bash
curl https://mise.run | sh                                # one-time
eval "$(~/.local/bin/mise activate bash)"                 # activate in current shell
echo 'eval "$(~/.local/bin/mise activate bash)"' >> ~/.bashrc   # persist

git clone https://github.com/tosin2013/aegis-node.git
cd aegis-node
mise install                                              # Rust 1.85, Go 1.23, cosign, node per mise.toml
cargo install --locked --path crates/cli --features llama
aegis identity init --trust-domain aegis-node.local
```

If you skip the `eval` step you'll see *"cargo: command not found"*
even after `mise install` succeeds — installing tools and putting
them on PATH are separate operations in mise.

## Native via system packages + rustup

For users who'd rather use distro packages + [rustup](https://rustup.rs)
directly instead of mise. The exact versions you get depend on your
distro's repos; Aegis-Node's CI tests against Rust 1.85+ and Go 1.23+.

```bash
# install OS prereqs (see per-OS section below)
# install rustup + Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source ~/.cargo/env

git clone https://github.com/tosin2013/aegis-node.git
cd aegis-node
cargo install --locked --path crates/cli --features llama
aegis identity init --trust-domain aegis-node.local
```

---

## Per-OS prerequisites

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

**Install Go** if not using mise (Go 1.23+ required; the apt `golang-go`
package may be too old on 22.04):

```bash
# from go.dev/dl
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

**Note on EPEL:** `jq` is in the base CentOS 10 repos. If a future
package goes missing, enable EPEL:

```bash
sudo dnf install -y epel-release
```

**Install `oras`** (no dnf package):

```bash
curl -fsSL "https://github.com/oras-project/oras/releases/download/v1.2.1/oras_1.2.1_linux_amd64.tar.gz" \
    | sudo tar -xz -C /usr/local/bin oras
```

**Install Go** if not using mise (Go 1.23+ required; the dnf `golang`
package version varies):

```bash
curl -fsSL "https://go.dev/dl/go1.23.4.linux-amd64.tar.gz" \
    | sudo tar -xz -C /usr/local
export PATH=/usr/local/go/bin:$PATH
echo 'export PATH=/usr/local/go/bin:$PATH' >> ~/.bashrc
```

**Install `cosign`** if not using mise:

```bash
curl -fsSL -o /tmp/cosign \
    "https://github.com/sigstore/cosign/releases/download/v2.4.1/cosign-linux-amd64"
sudo install -m 0755 /tmp/cosign /usr/local/bin/cosign
```

---

## Per-example extras (all OSes)

After the toolchain is installed, the per-example dependencies:

```bash
# Example 02 default mode (filesystem MCP)
npm install -g @modelcontextprotocol/server-filesystem

# Example 06 (SQLite MCP via uvx)
pipx install uv
```

## Verify the install

Same on every OS. Run Example 01 end-to-end:

```bash
cd /path/to/aegis-node/examples/01-hello-world
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

## Troubleshooting

### `cargo: command not found` after `mise install`

mise installed the tools but didn't add them to PATH. Run
`eval "$(~/.local/bin/mise activate bash)"` in your current shell
and add it to `~/.bashrc` for persistence.

### `feature edition2024 is required` from cargo

Your Rust is older than 1.85. The locked dependencies in `Cargo.lock`
need Rust 1.85+. Either run `mise install` again after pulling latest
(mise.toml pins 1.85.0), or upgrade your rustup toolchain:
`rustup update stable`.

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
