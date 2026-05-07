/**
 * Live `aegis validate` integration for the Manifest Builder.
 *
 * Posts the editor's YAML buffer to `POST /api/v1/manifests/validate`,
 * which shells out to the Go validator binary (per ADR-002 split-
 * language) and returns a structured findings array. The hook
 * defined here debounces edits, cancels in-flight requests when a
 * newer edit arrives, and exposes a `findings` + `summary` shape
 * the editor renders as Monaco markers.
 *
 * Validator semantics worth knowing:
 *
 *   - `severity` values: `"error"` | `"warn"` | `"info"`. Only
 *     `error` causes `ok=false`; warnings + info are advisory.
 *   - `line` and `col` are 0 today (the validator's YAML AST
 *     hookup is a follow-up). Markers fall back to line 1 col 1
 *     until the validator emits real positions.
 *   - The `binary` field tells the operator which validator
 *     produced the findings (env var override / dev path / PATH).
 */

import { useEffect, useRef, useState } from "react";

export type FindingSeverity = "error" | "warn" | "info" | string;

export interface Finding {
  rule_id: string;
  severity: FindingSeverity;
  field: string;
  message: string;
  rationale?: string;
  line: number;
  col: number;
}

export interface ValidateResponse {
  ok: boolean;
  findings: Finding[];
  binary: string;
}

export interface ValidateSummary {
  errors: number;
  warnings: number;
  infos: number;
}

export type ValidateState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "ready"; response: ValidateResponse; summary: ValidateSummary }
  | { kind: "error"; message: string };

export function tally(findings: Finding[]): ValidateSummary {
  const out: ValidateSummary = { errors: 0, warnings: 0, infos: 0 };
  for (const f of findings) {
    if (f.severity === "error") out.errors++;
    else if (f.severity === "warn") out.warnings++;
    else if (f.severity === "info") out.infos++;
  }
  return out;
}

const DEFAULT_DEBOUNCE_MS = 400;

/**
 * React hook: validate `yaml` after the operator stops typing for
 * `debounceMs` (default 400 ms). In-flight requests are cancelled
 * when a newer edit arrives, preventing race conditions where a
 * stale validate response overwrites the markers for current YAML.
 */
export function useValidate(
  yaml: string,
  options?: { debounceMs?: number; enabled?: boolean },
): ValidateState {
  const [state, setState] = useState<ValidateState>({ kind: "idle" });
  const debounceMs = options?.debounceMs ?? DEFAULT_DEBOUNCE_MS;
  const enabled = options?.enabled ?? true;

  const inflightRef = useRef<AbortController | null>(null);

  useEffect(() => {
    if (!enabled) return;
    if (yaml.trim().length === 0) {
      setState({ kind: "idle" });
      return;
    }

    const timer = setTimeout(() => {
      // Cancel any prior request before starting the next one.
      inflightRef.current?.abort();
      const controller = new AbortController();
      inflightRef.current = controller;

      setState({ kind: "loading" });

      fetch("/api/v1/manifests/validate", {
        method: "POST",
        headers: { "Content-Type": "application/x-yaml" },
        body: yaml,
        signal: controller.signal,
      })
        .then(async (r) => {
          if (!r.ok) {
            const text = await r.text().catch(() => `HTTP ${r.status}`);
            throw new Error(text || `HTTP ${r.status}`);
          }
          return (await r.json()) as ValidateResponse;
        })
        .then((response) => {
          if (controller.signal.aborted) return;
          setState({
            kind: "ready",
            response,
            summary: tally(response.findings),
          });
        })
        .catch((e: unknown) => {
          if (controller.signal.aborted) return;
          // AbortError surfaces here too; ignore those.
          if (e instanceof DOMException && e.name === "AbortError") return;
          setState({
            kind: "error",
            message: e instanceof Error ? e.message : String(e),
          });
        });
    }, debounceMs);

    return () => {
      clearTimeout(timer);
    };
  }, [yaml, debounceMs, enabled]);

  return state;
}
