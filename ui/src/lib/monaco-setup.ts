/**
 * Monaco airgap configuration. By default, `@monaco-editor/react`
 * loads Monaco from the cdn.jsdelivr.net CDN at runtime — that
 * works on internet-connected hosts but breaks airgap deployments
 * (the v0.9.5 release target per ADR-031 §"Localhost-only" + the
 * v1.0.0 CMMC posture).
 *
 * This module imports the `monaco-editor` ESM bundle and registers
 * it with `@monaco-editor/react`'s loader so the editor uses the
 * locally-bundled copy. Vite's `?worker` syntax inlines the Monaco
 * web workers as code-split chunks, so they load from the
 * Aegis-Node embedded assets, not a third-party origin.
 *
 * Sub-phase 1d.1d will add `monaco-yaml` for schema-aware
 * autocomplete + diagnostics; for now we get Monaco's built-in
 * YAML syntax highlighting plus the JSON worker (used by some
 * Monaco internals).
 *
 * Calling `configureMonaco()` is idempotent — safe to invoke from
 * multiple components if needed.
 */
import * as monaco from "monaco-editor";
import { loader } from "@monaco-editor/react";
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import JsonWorker from "monaco-editor/esm/vs/language/json/json.worker?worker";

let configured = false;

interface MonacoEnvironment {
  getWorker(_workerId: string, label: string): Worker;
}

interface WindowWithMonacoEnv extends Window {
  MonacoEnvironment?: MonacoEnvironment;
}

export function configureMonaco() {
  if (configured) return;
  configured = true;

  (self as unknown as WindowWithMonacoEnv).MonacoEnvironment = {
    getWorker(_workerId: string, label: string): Worker {
      if (label === "json") return new JsonWorker();
      return new EditorWorker();
    },
  };

  loader.config({ monaco });
}
