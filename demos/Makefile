# Aegis-Node demo program (per ADR-020).
#
# `make all` re-renders every demo by running VHS against each
# `<dir>/demo.tape`. Each demo's `prompt.txt` and `manifest.yaml` are
# the inputs; `demo.gif` is the regenerable output (committed; CI
# verifies SHA-256 matches).
#
# Determinism: each manifest sets `inference.determinism.seed = 42`
# and `temperature = 0.0`, so the model produces byte-identical output
# across runs. That + VHS's deterministic terminal renderer is what
# makes `make all && git status` come back clean (modulo legitimate
# changes a contributor intended).
#
# Per LLM-C, the underlying CLI must be built with `--features llama`:
#   cargo install --path ../crates/cli --features llama
# The Makefile invokes `aegis` from $PATH; if it's not the
# llama-featured build, the demos error out at `aegis run --prompt`
# with the missing-feature hint.

VHS ?= vhs
AEGIS ?= aegis

# `find -mindepth 2 -name demo.tape` discovers every demo. Skip
# `_template/` so the placeholder doesn't render.
TAPES := $(shell find . -mindepth 2 -name demo.tape -not -path './_template/*' | sort)
GIFS  := $(TAPES:demo.tape=demo.gif)

.PHONY: all clean check-tools $(notdir $(patsubst %/,%,$(dir $(TAPES))))

all: check-tools $(GIFS)

# Per-demo targets named after the directory (e.g. `make 01-mcp-sandboxed-twice`).
# Each is an alias to the corresponding `demo.gif` rule.
$(notdir $(patsubst %/,%,$(dir $(TAPES)))): %: %/demo.gif

%.gif: %.tape
	@echo "==> rendering $<"
	@if [ -x $(dir $<)setup.sh ]; then \
		echo "==> running $(dir $<)setup.sh"; \
		$(dir $<)setup.sh; \
	fi
	cd $(dir $<) && $(VHS) $(notdir $<)

clean:
	@for d in $(dir $(TAPES)); do \
		rm -f $$d/demo.gif $$d/demo.cast $$d/*.png 2>/dev/null || true; \
	done

check-tools:
	@command -v $(VHS) >/dev/null || { \
		echo "ERROR: $(VHS) not found on PATH."; \
		echo "Install: go install github.com/charmbracelet/vhs@latest"; \
		exit 2; \
	}
	@command -v $(AEGIS) >/dev/null || { \
		echo "ERROR: $(AEGIS) not found on PATH."; \
		echo "Install: cargo install --path ../crates/cli --features llama"; \
		exit 2; \
	}
	@$(AEGIS) --help 2>&1 | grep -q -- '--prompt' || { \
		echo "ERROR: 'aegis' on PATH was built without the 'llama' feature ('--prompt' missing)."; \
		echo "Reinstall: cargo install --path ../crates/cli --features llama --force"; \
		exit 2; \
	}
