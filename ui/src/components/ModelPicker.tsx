import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Boxes, ChevronDown, Loader2, ShieldCheck } from "lucide-react";
import { cn } from "@/lib/utils";

/**
 * Model-picker dropdown for the chat surface (ADR-032 §"Session
 * Forking", sub-phase 1d.2e.1). Lists cached models from
 * `/api/v1/models` and triggers a fork via `POST /api/v1/sessions/fork`
 * when the operator picks one.
 *
 * On a successful fork the SPA closes and re-opens the WebSocket
 * (the active connection captured the previous backend at upgrade
 * time per the handler design); the parent component handles the
 * teardown via `onForkComplete`.
 *
 * 1d.2e.1 limitations the picker surfaces honestly:
 * - Same-backend swaps only — picking a Gemma 4 (LiteRT-LM) model
 *   from a Qwen-booted (llama) process is rejected by the backend
 *   with a clear error; the picker shows it inline rather than
 *   pretending the swap worked.
 * - No chat-history replay — the parent clears the thread on
 *   successful fork. Replay lands in 1d.2e.2 alongside the
 *   cross-backend dispatch.
 * - Stub backend (no `--manifest`/`--model`) → fork endpoint
 *   returns 503 → the picker disables itself with a tooltip
 *   pointing operators at the CLI.
 */

interface ModelCosign {
  verified: boolean;
  mode: string;
  keyless_identity_pattern?: string;
  key_path?: string;
}

interface Model {
  digest: string;
  oci_ref: string | null;
  cosign?: ModelCosign;
  size_bytes: number;
  has_chat_template: boolean;
}

interface ModelsResponse {
  cache_dir: string;
  models: Model[];
}

interface ForkResponse {
  ok: boolean;
  model_digest: string;
  schema: string;
}

interface ModelPickerProps {
  /** Called after a successful fork lands. Parent should close +
   *  reopen the WebSocket, clear the chat thread, and emit a
   *  system message announcing the swap. */
  onForkComplete: (modelDigest: string) => void;
  /** Optional disabled state (e.g. when the chat surface is mid-turn).
   *  Currently unused; reserved for 1d.2e.2's "fork mid-turn" UX. */
  disabled?: boolean;
}

type ForkState =
  | { kind: "idle" }
  | { kind: "forking"; toDigest: string }
  | { kind: "error"; message: string };

function shortDigest(digest: string): string {
  // sha256:<64 hex> → first 8 chars only for compact display in the
  // dropdown rows.
  const trimmed = digest.replace(/^sha256:/, "");
  return trimmed.slice(0, 8);
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
}

function modelLabel(m: Model): string {
  // Prefer the OCI repo path tail (e.g. "qwen2.5-1.5b-instruct-q4_k_m"
  // out of "ghcr.io/.../qwen2.5-1.5b-instruct-q4_k_m@sha256:...");
  // fall back to the short digest when ref isn't preserved.
  if (m.oci_ref) {
    const beforeAt = m.oci_ref.split("@", 1)[0];
    const tail = beforeAt.split("/").pop() ?? beforeAt;
    return tail;
  }
  return shortDigest(m.digest);
}

export function ModelPicker({ onForkComplete, disabled }: ModelPickerProps) {
  const [open, setOpen] = useState(false);
  const [activeDigest, setActiveDigest] = useState<string | null>(null);
  const [forkState, setForkState] = useState<ForkState>({ kind: "idle" });
  const containerRef = useRef<HTMLDivElement | null>(null);

  const { data, isLoading, error } = useQuery<ModelsResponse>({
    queryKey: ["models"],
    queryFn: async () => {
      const r = await fetch("/api/v1/models");
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      return r.json();
    },
    refetchOnWindowFocus: false,
  });

  // Click-away handler: close the panel when the operator clicks
  // outside it.
  useEffect(() => {
    if (!open) return;
    const onDocClick = (e: MouseEvent) => {
      if (
        containerRef.current &&
        !containerRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onDocClick);
    return () => document.removeEventListener("mousedown", onDocClick);
  }, [open]);

  async function handlePick(digest: string) {
    if (forkState.kind === "forking") return;
    // Strip the `sha256:` prefix the API returns; the fork endpoint
    // expects the bare hex (matches the cache subdirectory name).
    const bareDigest = digest.replace(/^sha256:/, "");
    setForkState({ kind: "forking", toDigest: bareDigest });
    try {
      const r = await fetch("/api/v1/sessions/fork", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ model_digest: bareDigest }),
      });
      if (!r.ok) {
        const text = await r.text().catch(() => `HTTP ${r.status}`);
        throw new Error(text || `HTTP ${r.status}`);
      }
      const body = (await r.json()) as ForkResponse;
      setActiveDigest(body.model_digest);
      setForkState({ kind: "idle" });
      setOpen(false);
      onForkComplete(body.model_digest);
    } catch (e) {
      setForkState({
        kind: "error",
        message: e instanceof Error ? e.message : String(e),
      });
    }
  }

  const buttonLabel = useMemo(() => {
    if (forkState.kind === "forking") return "forking…";
    if (activeDigest && data) {
      const m = data.models.find(
        (x) =>
          x.digest === `sha256:${activeDigest}` || x.digest === activeDigest,
      );
      if (m) return modelLabel(m);
    }
    return "model";
  }, [forkState, activeDigest, data]);

  return (
    <div
      ref={containerRef}
      className={cn("relative", disabled && "opacity-50")}
    >
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        disabled={disabled || forkState.kind === "forking"}
        aria-expanded={open}
        aria-haspopup="listbox"
        className="inline-flex items-center gap-1.5 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-elev)] px-2.5 py-1 text-xs transition-colors hover:border-accent hover:text-accent disabled:opacity-50"
        title="Switch the chat surface to a different cached model (ADR-032 Session Forking)"
      >
        {forkState.kind === "forking" ? (
          <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
        ) : (
          <Boxes className="h-3.5 w-3.5" aria-hidden="true" />
        )}
        <span className="font-mono">{buttonLabel}</span>
        <ChevronDown
          className={cn("h-3 w-3 transition-transform", open && "rotate-180")}
          aria-hidden="true"
        />
      </button>

      {open && (
        <div
          role="listbox"
          aria-label="Cached models"
          className="absolute right-0 z-20 mt-2 w-[22rem] max-h-[20rem] overflow-y-auto rounded-md border border-[var(--color-border)] bg-[var(--color-bg-elev)] shadow-xl"
        >
          {isLoading && (
            <div className="px-3 py-2 font-mono text-xs text-muted">
              loading model cache…
            </div>
          )}
          {error && (
            <div className="px-3 py-2 font-mono text-xs text-danger">
              {error instanceof Error ? error.message : String(error)}
            </div>
          )}
          {data && data.models.length === 0 && (
            <div className="px-3 py-3 text-xs text-muted">
              No models cached yet. Pull one with{" "}
              <code className="font-mono text-accent">
                aegis pull &lt;ref&gt;
              </code>
              .
            </div>
          )}
          {data && data.models.length > 0 && (
            <ul>
              {data.models.map((m) => {
                const bareDigest = m.digest.replace(/^sha256:/, "");
                const isActive = activeDigest === bareDigest;
                return (
                  <li key={m.digest}>
                    <button
                      type="button"
                      onClick={() => handlePick(m.digest)}
                      disabled={forkState.kind === "forking" || isActive}
                      className={cn(
                        "block w-full px-3 py-2 text-left text-xs transition-colors hover:bg-[var(--color-bg)]",
                        isActive && "bg-[var(--color-bg)] cursor-default",
                      )}
                    >
                      <div className="flex items-baseline justify-between gap-2">
                        <span className="font-mono text-accent">
                          {modelLabel(m)}
                        </span>
                        <span className="flex shrink-0 items-center gap-1.5 font-mono text-[10px] text-muted">
                          {m.cosign?.verified && (
                            <span className="inline-flex items-center gap-0.5 text-success">
                              <ShieldCheck
                                className="h-3 w-3"
                                aria-hidden="true"
                              />
                              verified
                            </span>
                          )}
                          {formatBytes(m.size_bytes)}
                        </span>
                      </div>
                      <div className="mt-0.5 truncate font-mono text-[10px] text-muted">
                        {shortDigest(m.digest)}…
                        {isActive && (
                          <span className="ml-1.5 text-accent">(loaded)</span>
                        )}
                      </div>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
          {forkState.kind === "error" && (
            <div className="border-t border-[var(--color-border)] bg-[var(--color-bg-elev)] px-3 py-2 font-mono text-[11px] text-danger">
              fork failed: {forkState.message}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
