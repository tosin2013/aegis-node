# Aegis-Node devbox per ADR-017.
# Provides Rust + Go + supporting tooling for development and CI.
# This Dockerfile is the source for the signed OCI artifact published as
# ghcr.io/<owner>/aegis-node-devbox.
#
# Tool versions are kept aligned with mise.toml at the repo root.

FROM mcr.microsoft.com/devcontainers/base:ubuntu-22.04

ARG TARGETARCH

ARG GO_VERSION=1.23.4
ARG RUST_VERSION=1.86.0
ARG BUF_VERSION=1.47.2
ARG GOLANGCI_LINT_VERSION=1.62.2
ARG COSIGN_VERSION=2.4.1
ARG ORAS_VERSION=1.2.1
ARG PROTOC_VERSION=28.3
ARG NODE_MAJOR=20

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        clang \
        cmake \
        curl \
        git \
        jq \
        pkg-config \
        unzip \
    && rm -rf /var/lib/apt/lists/*

# Node.js (for ajv-cli)
RUN curl -fsSL "https://deb.nodesource.com/setup_${NODE_MAJOR}.x" | bash - \
 && apt-get install -y --no-install-recommends nodejs \
 && rm -rf /var/lib/apt/lists/* \
 && npm install -g ajv-cli@5.0.0

# Go
RUN curl -fsSL "https://go.dev/dl/go${GO_VERSION}.linux-${TARGETARCH}.tar.gz" \
    | tar -C /usr/local -xz \
 && ln -s /usr/local/go/bin/go /usr/local/bin/go \
 && ln -s /usr/local/go/bin/gofmt /usr/local/bin/gofmt

# Rust toolchain (system-wide)
ENV CARGO_HOME=/usr/local/cargo \
    RUSTUP_HOME=/usr/local/rustup \
    PATH=/usr/local/cargo/bin:${PATH}
RUN curl -fsSL https://sh.rustup.rs \
    | sh -s -- -y \
        --default-toolchain "${RUST_VERSION}" \
        --component clippy \
        --component rustfmt \
        --profile minimal \
 && chmod -R a+rwX /usr/local/cargo /usr/local/rustup

# protoc
RUN PROTOC_ARCH=$(case "$TARGETARCH" in amd64) echo x86_64 ;; arm64) echo aarch_64 ;; *) echo "$TARGETARCH" ;; esac) \
 && curl -fsSL -o /tmp/protoc.zip "https://github.com/protocolbuffers/protobuf/releases/download/v${PROTOC_VERSION}/protoc-${PROTOC_VERSION}-linux-${PROTOC_ARCH}.zip" \
 && unzip -d /usr/local /tmp/protoc.zip "bin/*" "include/*" \
 && rm /tmp/protoc.zip \
 && chmod -R a+rX /usr/local/include

# buf
RUN BUF_ARCH=$(case "$TARGETARCH" in amd64) echo x86_64 ;; arm64) echo aarch64 ;; *) echo "$TARGETARCH" ;; esac) \
 && curl -fsSL -o /usr/local/bin/buf "https://github.com/bufbuild/buf/releases/download/v${BUF_VERSION}/buf-Linux-${BUF_ARCH}" \
 && chmod +x /usr/local/bin/buf

# golangci-lint
RUN curl -fsSL "https://github.com/golangci/golangci-lint/releases/download/v${GOLANGCI_LINT_VERSION}/golangci-lint-${GOLANGCI_LINT_VERSION}-linux-${TARGETARCH}.tar.gz" \
    | tar -xz -C /tmp \
 && mv "/tmp/golangci-lint-${GOLANGCI_LINT_VERSION}-linux-${TARGETARCH}/golangci-lint" /usr/local/bin/ \
 && rm -rf "/tmp/golangci-lint-${GOLANGCI_LINT_VERSION}-linux-${TARGETARCH}"

# cosign
RUN curl -fsSL -o /usr/local/bin/cosign "https://github.com/sigstore/cosign/releases/download/v${COSIGN_VERSION}/cosign-linux-${TARGETARCH}" \
 && chmod +x /usr/local/bin/cosign

# oras
RUN curl -fsSL "https://github.com/oras-project/oras/releases/download/v${ORAS_VERSION}/oras_${ORAS_VERSION}_linux_${TARGETARCH}.tar.gz" \
    | tar -xz -C /usr/local/bin oras

USER vscode
ENV PATH=/usr/local/cargo/bin:${PATH}
