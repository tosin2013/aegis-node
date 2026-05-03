# Demo 05 — Tampered model halts session (F1 IdentityRebind)

Per [ADR-020](../../docs/adrs/020-recorded-demo-program.md) §"Six
scenarios" item 5. The 30-second clip shows that an attacker who
swaps a model's bytes mid-session can't slip the change past the F1
identity binding: the per-tool-call rebind re-hashes the model file
on disk and the SVID-bound digest no longer matches.

## What this demonstrates

| Layer | Behavior in this demo |
|---|---|
| **F1 Workload Identity** | Session boots, sha256s `model.gguf`, issues an SVID with that digest bound (per [ADR-003](../../docs/adrs/003-cryptographic-workload-identity-spiffe-spire.md)). |
| **F1 per-tool-call rebind** | Every `mediate_*` re-hashes the model file before allowing the call. Fresh sha ≠ bound sha → halt + Violation entry naming the field that drifted. |
| **F9 Hash-chained ledger** | The Violation lands in the ledger as a tamper-evident entry; `aegis verify` still reports chain-intact, so an auditor can trust the violation record itself. |

## What you see in the GIF

| t | Frame | Note |
|---|---|---|
| 0-2s | sha256 of the original `model.gguf` | The 1.1 GB Qwen2.5-1.5B-Q4 blob |
| 2-3s | `aegis run --prompt "..."` launches in the background | Session boot completes quickly; model load via mmap |
| 3-4s | `mv /tmp/t.bin model.gguf` rotates the file | Atomic rename: llama.cpp's mmap'd pages stay valid (no SIGBUS); the path now resolves to a 14-byte tampered file |
| 4-5s | sha256 of `model.gguf` is now different | The on-disk file is "tampered bytes\n" |
| 5-8s | `wait` returns with `aegis exit: 1` | Mediator's rebind fired on the model's first tool call |
| 8-14s | `jq` shows the Violation entry | `violationReason: identity digest binding violated: model digest changed (bound=6a1a..., live=92e7...)` |

## Why atomic rename, not truncate

llama.cpp loads the GGUF via `mmap` (per `llama-cpp-2`'s default).
Truncating the file with `>` shrinks the underlying inode below the
mmap'd extent, which causes SIGBUS the next time llama.cpp touches a
post-truncation page. An atomic `mv new old` keeps the *old inode*
alive (the mmap holds it; it's unlinked but referenced) while the
*path* `model.gguf` now resolves to a new inode.

The mediator's rebind reads via `File::open(model_path)`, which
performs a fresh path resolution → reads the new (tampered) inode →
fresh sha256 ≠ bound sha → trip.

This is also a real-world attack pattern: an attacker with write
access to the model directory rotates the model with `mv` to avoid
breaking the running process. Aegis-Node catches it.

## Run locally

```bash
make -C demos 05-tampered-model-halts
```

That single command runs `setup.sh` (one-time-per-machine model
pull, then no-op) and renders the demo. Prerequisites: `aegis` CLI
built with `--features llama`, plus `oras` and `cosign`.

### What `setup.sh` does

1. `aegis pull` the cosign-verified Qwen 2.5 1.5B Q4_K_M GGUF (cached
   at `~/.cache/aegis/models/c7404a910e65596a185e788ede19e09bc017dc3101cd106ba7d65fe1dd7dec37/`).
2. *Copy* (not symlink) the model into `/tmp/aegis-demo-05/model.gguf`
   so the demo's atomic-rename tamper (`mv /tmp/t.bin model.gguf`)
   doesn't replace the cached blob.
3. Symlink the chat-template sidecar + `manifest.yaml` into the
   workdir (the latter so `demo.tape` can use a workdir-local path).

## Reproducibility

Per ADR-020 hard requirements, `manifest.yaml` pins
`inference.determinism` (seed 42 + temperature 0). The model output
on the first turn is byte-identical across renders. The tamper
timing is the only remaining race — `Sleep 1s` after launch, on a
machine where Qwen 1.5B Q4 finishes its first turn in ~3s, lands the
swap reliably in the window.

If your hardware is unusual (e.g. 16+ cores with very fast inference
or an emulated CPU) the model may finish before the swap. Bump
`Sleep` durations in `demo.tape` and re-render.

## Why the tape uses absolute paths

The .tape's `aegis run --manifest $DEMO_MANIFEST` references this
directory's `manifest.yaml` via an absolute path
(`/root/aegis-node/...`) so the recording is portable across users'
checkout locations as long as `aegis-node` lives at that path. Adapt
if your checkout is elsewhere.
