.PHONY: all build build-go build-rust test test-go test-rust lint lint-go lint-rust proto schema-check clean help replay-viewer-open replay-viewer-check

# Phase 0 substrate per ADR-002. proto/ and schemas/ targets are placeholders
# until the Phase 0 schema tasks (aegis.proto, manifest JSON Schema, ledger
# JSON-LD context) land.

GO ?= go
CARGO ?= cargo

all: build

help: ## Show available targets
	@awk 'BEGIN{FS=":.*## "}/^[a-zA-Z_-]+:.*## /{printf "  %-18s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

build: build-go build-rust ## Build everything (Go + Rust)

build-go:
	$(GO) build ./...

build-rust:
	$(CARGO) build --workspace

test: test-go test-rust ## Run all tests (Go + Rust)

test-go:
	$(GO) test -race -count=1 ./...

test-rust:
	$(CARGO) test --workspace

lint: lint-go lint-rust ## Run all linters

lint-go:
	$(GO) vet ./...
	@command -v golangci-lint >/dev/null 2>&1 && golangci-lint run ./... || echo "golangci-lint not installed; skipping"

lint-rust:
	$(CARGO) fmt --all -- --check
	$(CARGO) clippy --workspace --all-targets -- -D warnings

proto: ## Regenerate protobuf stubs (pending aegis.proto)
	@echo "proto regeneration: pending aegis.proto (Phase 0 schema task, ADR-002)"

schema-check: ## Validate JSON Schema + JSON-LD (pending schemas/)
	@echo "schema validation: pending schemas/ (Phase 0 schema task, ADR-004 + ADR-011)"

replay-viewer-open: ## Open the offline replay viewer in $BROWSER (xdg-open / open)
	@if command -v xdg-open >/dev/null 2>&1; then \
		xdg-open tools/replay-viewer/index.html; \
	elif command -v open >/dev/null 2>&1; then \
		open tools/replay-viewer/index.html; \
	else \
		echo "open tools/replay-viewer/index.html in your browser"; \
	fi

replay-viewer-check: ## Enforce ADR-010 air-gap rules on the viewer
	@./tools/replay-viewer/check-airgap.sh

clean: ## Clean build outputs
	$(GO) clean ./...
	$(CARGO) clean
