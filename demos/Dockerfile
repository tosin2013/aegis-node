# Aegis-Node demo renderer image (per ADR-020 §"Phase 2b CI snapshot").
#
# Builds the hermetic environment that produces the committed GIFs in
# demos/<scenario>/demo.gif. The CI snapshot test (.github/workflows/
# demos.yml) renders each deterministic demo (1, 5, 6) inside this
# image and asserts SHA-256 against the committed GIF, gating drift on
# every PR that touches demos/**.
#
# Pinning rationale: GIF SHA is a function of (model output bytes) +
# (terminal renderer bytes) + (font metrics). Model output is locked
# by Qwen2.5 + seed=42 + temperature=0; the renderer bytes depend on
# vhs + chrome + ttyd + ffmpeg + the font; the font is fonts-firacode
# from ubuntu noble, locked by the base image's apt snapshot.
#
# Versions match the image that produced the currently-committed GIFs
# (verified against `docker history aegis-demo-renderer:latest` from
# the bdb896e/9193e05 render run).
#
# Demos 2/3/4 use the LiteRT-LM backend + Gemma 4; their CPU sampler
# is upstream-broken (LiteRT-LM #2080) so they fall back to non-
# deterministic baked sampler params and are excluded from the snap-
# shot test until upstream lands the fix. This image still supports
# rendering them by hand for documentation purposes.

FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

# Base toolchain. fonts-firacode is what the .tape files render with;
# changing it changes every committed GIF SHA. ttyd + ffmpeg are vhs's
# transitive dependencies. Node 20 (installed below from NodeSource)
# is needed for the upstream MCP filesystem server (Demo 1) and for
# the SPA build under `ui/` per ADR-031 — Ubuntu 24.04's apt `npm`
# package pulls Node 18, which is too old for Vite 6 + Tailwind 4
# (`@tailwindcss/oxide` native binding fails to load under Node 18).
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl wget git gnupg \
        build-essential cmake libclang-dev clang lld \
        pkg-config \
        ffmpeg ttyd fonts-firacode \
        jq \
        gdb \
    && rm -rf /var/lib/apt/lists/*

# Node 20 from NodeSource — Vite 6 + Tailwind 4 (per ADR-031's
# locked UI stack) require Node >= 20, and Ubuntu 24.04's apt repo
# only ships Node 18. The setup script adds NodeSource's apt source;
# the subsequent install pulls a Node-20 + matching-npm bundle.
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Chrome (vhs renders its terminal frames through headless chrome).
# Pinned to the apt-repo's `_current` for now — Google doesn't keep
# old .deb versions, so this floats with whatever chrome stable is
# at image build time. If GIF SHAs drift between renderer-image
# rebuilds, lock to a snapshot mirror or vendor a specific .deb.
RUN curl -fsSLo /tmp/chrome.deb \
      https://dl.google.com/linux/direct/google-chrome-stable_current_amd64.deb \
    && apt-get update \
    && apt-get install -y /tmp/chrome.deb \
    && rm /tmp/chrome.deb \
    && rm -rf /var/lib/apt/lists/* \
    && printf '#!/bin/sh\nexec /opt/google/chrome/google-chrome --no-sandbox "$@"\n' \
         > /usr/local/bin/google-chrome \
    && chmod +x /usr/local/bin/google-chrome

# OCI tooling — `aegis pull` shells out to oras for the artifact
# fetch and to cosign for the keyless verify.
RUN curl -fsSLo /tmp/oras.tar.gz \
      https://github.com/oras-project/oras/releases/download/v1.2.0/oras_1.2.0_linux_amd64.tar.gz \
    && tar -xzf /tmp/oras.tar.gz -C /usr/local/bin oras \
    && rm /tmp/oras.tar.gz \
    && curl -fsSLo /usr/local/bin/cosign \
      https://github.com/sigstore/cosign/releases/download/v2.4.1/cosign-linux-amd64 \
    && chmod +x /usr/local/bin/cosign

# Rust toolchain — pinned to the version that produced the current
# committed GIFs. Cargo cache lives in $CARGO_HOME (default
# /root/.cargo); the demos.yml workflow restores it across runs.
RUN curl -fsSL https://sh.rustup.rs | sh -s -- \
        -y --default-toolchain 1.95.0 --profile minimal \
    && /root/.cargo/bin/rustup component add rustfmt clippy

# Go (for `vhs`). 1.23.4 matches the version that produced the
# committed GIFs; bumping go bumps the vhs binary which can affect
# rendering if vhs's deps change.
RUN curl -fsSLo /tmp/go.tar.gz \
      https://go.dev/dl/go1.23.4.linux-amd64.tar.gz \
    && tar -xzf /tmp/go.tar.gz -C /usr/local \
    && rm /tmp/go.tar.gz

ENV PATH=/root/.cargo/bin:/usr/local/go/bin:/root/go/bin:/usr/local/bin:/usr/bin:/bin

# vhs v0.11.0 — pinned. Newer vhs releases occasionally change frame
# encoding subtly; locking to v0.11.0 keeps GIF SHAs stable.
RUN go install github.com/charmbracelet/vhs@v0.11.0

# MCP filesystem server (Anthropic) — Demo 1 uses this. The
# demo manifests reference /usr/bin/mcp-server-filesystem; the
# install location depends on the npm distribution:
#
#   - NodeSource Node 20 (this image): npm prefix=/usr → binary
#     at /usr/bin/mcp-server-filesystem (already at the expected
#     location, no symlink needed)
#   - Legacy apt npm: prefix=/usr/local → binary at
#     /usr/local/bin/mcp-server-filesystem (symlink needed)
#
# The conditional symlink below handles both, and explicitly
# avoids the `ln -sf` footgun that overwrote npm's working
# /usr/bin link with a broken /usr/local target on Node 20.
RUN npm install -g @modelcontextprotocol/server-filesystem \
    && actual="$(command -v mcp-server-filesystem)" \
    && test -n "$actual" \
    && if [ "$actual" != "/usr/bin/mcp-server-filesystem" ]; then \
         ln -sf "$actual" /usr/bin/mcp-server-filesystem; \
       fi \
    && test -x /usr/bin/mcp-server-filesystem

# pnpm — required by `crates/ui-server`'s build.rs (per ADR-031). The
# demo build path runs `cargo build --release -p aegis-cli --features
# llama` inside this image, which transitively triggers the SPA
# build script. Pinned to pnpm@10 to match `ui/package.json`'s
# `packageManager` field. Runs against the Node 20 install above —
# Tailwind 4's `@tailwindcss/oxide` native binding fails to resolve
# under the bundled npm + Node 18 combination Ubuntu apt provides.
# See #142 for the corresponding host-side CI fixes (rust.yml /
# llama.yml / litertlm.yml).
RUN npm install -g pnpm@10

WORKDIR /work
CMD ["/bin/bash"]
