#!/bin/bash
# Aegis-Node native install — Ubuntu 22.04 / 24.04
#
# One-shot installer for the toolchain + aegis CLI. Mirrors
# docs/INSTALL.md "Step 1: OS prerequisites (Ubuntu)" → "Step 2a:
# Native via mise" → "Step 4: verify Example 01".
#
# Run from inside an aegis-node checkout:
#
#     cd aegis-node
#     bash scripts/install/ubuntu.sh
#
# Idempotent: re-runs the apt/mise/cargo steps (each is internally
# idempotent) and skips the identity-init step if the local CA
# already exists.
#
# Empirically validated against ubuntu:24.04 on 2026-05-04.

set -euo pipefail

# ============================================================
# Sanity checks
# ============================================================
if [ ! -f mise.toml ] || [ ! -d crates/cli ]; then
    echo "ERROR: run this from the aegis-node workspace root (the dir containing mise.toml + crates/)" >&2
    exit 2
fi

if ! command -v sudo >/dev/null 2>&1; then
    echo "ERROR: sudo not found — install sudo or run this script as a user with apt-get permission" >&2
    exit 2
fi

WORKSPACE="$(pwd)"

log() { printf '\n===== %s =====\n' "$*"; }

# ============================================================
# Step 1: Ubuntu prerequisites
# ============================================================
log "Step 1: apt-get prerequisites"
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
    curl ca-certificates git \
    build-essential pkg-config clang cmake unzip \
    jq sqlite3 \
    nodejs npm pipx

pipx ensurepath

# oras (no apt package; download tarball)
ORAS_VERSION=1.2.1
if ! command -v oras >/dev/null 2>&1; then
    log "Step 1: install oras ${ORAS_VERSION}"
    curl -fsSL "https://github.com/oras-project/oras/releases/download/v${ORAS_VERSION}/oras_${ORAS_VERSION}_linux_amd64.tar.gz" \
        | sudo tar -xz -C /usr/local/bin oras
fi

# ============================================================
# Step 2a: install mise + toolchain
# ============================================================
if ! command -v mise >/dev/null 2>&1 && [ ! -x "$HOME/.local/bin/mise" ]; then
    log "Step 2a: install mise"
    curl -fsSL https://mise.run | sh
fi
eval "$("$HOME/.local/bin/mise" activate bash)"

if ! grep -q 'mise activate bash' "$HOME/.bashrc" 2>/dev/null; then
    echo 'eval "$(~/.local/bin/mise activate bash)"' >> "$HOME/.bashrc"
fi

log "Step 2a: trust mise.toml"
mise trust "$WORKSPACE/mise.toml"

log "Step 2a: mise install (Rust 1.85, Go 1.23, cosign, node)"
mise install

# Activate cargo (mise's rust backend often defers to ~/.cargo/bin)
if [ -f "$HOME/.cargo/env" ]; then
    set +u; source "$HOME/.cargo/env"; set -u
fi

# Verify Rust 1.85+, fall back to rustup if mise pinned an older toolchain
RUST_MAJOR_MINOR=$(rustc --version | awk '{print $2}' | cut -d. -f1-2)
if [ "$(printf '%s\n%s\n' "$RUST_MAJOR_MINOR" "1.85" | sort -V | head -1)" != "1.85" ]; then
    log "Rust < 1.85 detected ($RUST_MAJOR_MINOR), upgrading via rustup"
    rustup install 1.85.0 && rustup default 1.85.0
    unset RUSTUP_TOOLCHAIN
fi
log "Rust toolchain: $(rustc --version)"

# ============================================================
# Step 2a: build + install aegis
# ============================================================
log "Step 2a: cargo build (workspace-aware; respects Cargo.lock)"
cargo build --release -p aegis-cli --features llama --locked

log "Step 2a: install aegis to ~/.local/bin"
mkdir -p "$HOME/.local/bin"
cp target/release/aegis "$HOME/.local/bin/aegis"
export PATH="$HOME/.local/bin:$PATH"
if ! grep -q 'PATH="\$HOME/.local/bin' "$HOME/.bashrc" 2>/dev/null; then
    echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.bashrc"
fi
aegis --version

# ============================================================
# Step 2a: identity init (idempotent — skip if CA already exists)
# ============================================================
CA_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/aegis/identity"
if [ ! -f "$CA_DIR/ca.crt" ]; then
    log "Step 2a: identity init"
    aegis identity init --trust-domain aegis-node.local
else
    log "Step 2a: identity CA already at $CA_DIR — skipping init"
fi

# ============================================================
# Step 3: per-example extras
# ============================================================
if ! command -v mcp-server-filesystem >/dev/null 2>&1; then
    log "Step 3: install mcp-server-filesystem (Example 02 default mode)"
    sudo npm install -g @modelcontextprotocol/server-filesystem
fi

if ! command -v uvx >/dev/null 2>&1; then
    log "Step 3: install uv (provides uvx; Example 06)"
    pipx install uv
fi

# ============================================================
# Step 4: verify with Example 01
# ============================================================
log "Step 4: verify with Example 01"
cd "$WORKSPACE/examples/01-hello-world"
bash setup.sh

cd /tmp/aegis-example-01
aegis run --manifest manifest.yaml --model model.gguf \
    --workload hello-world --instance inst-001 \
    --prompt "$(cat prompt.txt)"

if [ ! -s output/greeting.txt ]; then
    echo "ERROR: output/greeting.txt missing or empty" >&2
    exit 1
fi
echo
echo "Artifact: $(cat output/greeting.txt)"
echo

aegis verify ledger-*.jsonl

cat <<EOF


============================================================
Aegis-Node install complete.

  aegis CLI:       $(command -v aegis)
  Example 01 PASS: /tmp/aegis-example-01/output/greeting.txt

Continue with examples 02-06: see $WORKSPACE/examples/README.md
============================================================
EOF
