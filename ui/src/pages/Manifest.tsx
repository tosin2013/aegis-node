import { lazy, Suspense, useEffect, useRef, useState } from "react";
import type * as Monaco from "monaco-editor";
import { CircleAlert, FileCode, Info, Save, TriangleAlert } from "lucide-react";
import { toast } from "sonner";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { TemplatePicker } from "@/components/TemplatePicker";
import type { Template } from "@/templates/types";
import { useValidate, type ValidateState } from "@/lib/validate";

// Locally-bundled Monaco. The dynamic imports below — `monaco-setup`
// (which pulls in `monaco-editor` + the worker `?worker` chunks) and
// `@monaco-editor/react` — are deliberately deferred *inside* the
// lazy boundary so Vite splits them into their own chunk. If
// `monaco-setup` is imported at the top of this file, the entire
// ~4 MB Monaco core leaks into the eager bundle and blows the
// initial-route size budget. The current arrangement keeps Home +
// Models bundles light; only /manifest pays Monaco's cost.
const MonacoEditor = lazy(async () => {
  const setup = await import("@/lib/monaco-setup");
  setup.configureMonaco();
  const mod = await import("@monaco-editor/react");
  return { default: mod.default };
});

const FALLBACK_YAML = `# Aegis-Node permission manifest.
# Load a curated template from the dropdown above to get started, or
# hand-author here. Save writes the file the CLI consumes; live
# \`aegis validate\` diagnostics render inline as you type.

schemaVersion: "1"
agent:
  name: "my-agent"
  version: "1.0.0"
identity:
  spiffeId: "spiffe://aegis-node.local/agent/my-agent/inst-001"
tools:
  filesystem:
    read:
      - /path/to/your/data
  network:
    outbound: deny
    inbound: deny

inference:
  determinism:
    seed: 42
    temperature: 0.0
    top_p: 1.0
    top_k: 0
    repeat_penalty: 1.0
`;

interface SaveResponse {
  saved: boolean;
  path: string;
  bytes: number;
}

/**
 * Diagnostic owner string — Monaco scopes markers per (model URI,
 * owner). Using a stable owner means subsequent validate runs
 * REPLACE prior markers cleanly rather than accumulating.
 */
const VALIDATE_MARKER_OWNER = "aegis-validate";

export function Manifest() {
  const [yaml, setYaml] = useState<string>(FALLBACK_YAML);
  const [activeTemplateId, setActiveTemplateId] = useState<string | null>(
    null,
  );
  const [savedPath, setSavedPath] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);

  const validateState = useValidate(yaml);

  // Hold a ref to the underlying Monaco editor instance so the
  // marker-applying effect can call setModelMarkers on the active
  // model. The ref is set in onMount; it stays stable across re-
  // renders and survives hot-reload during dev.
  const editorRef = useRef<Monaco.editor.IStandaloneCodeEditor | null>(null);
  const monacoRef = useRef<typeof Monaco | null>(null);

  useEffect(() => {
    setDirty(yaml !== FALLBACK_YAML || savedPath !== null);
  }, [yaml, savedPath]);

  // Apply the latest validate findings as Monaco markers. Runs
  // every time validateState transitions; clears markers on idle
  // / loading / error so stale diagnostics don't linger when the
  // operator's still typing.
  useEffect(() => {
    const editor = editorRef.current;
    const monaco = monacoRef.current;
    if (!editor || !monaco) return;
    const model = editor.getModel();
    if (!model) return;

    if (validateState.kind !== "ready") {
      monaco.editor.setModelMarkers(model, VALIDATE_MARKER_OWNER, []);
      return;
    }

    const markers = validateState.response.findings.map((f) => {
      // Validator emits 0/0 today; fall back to highlighting line 1
      // until the YAML AST hookup ships precise positions.
      const startLine = f.line > 0 ? f.line : 1;
      const startCol = f.col > 0 ? f.col : 1;
      return {
        severity: monacoSeverity(monaco, f.severity),
        startLineNumber: startLine,
        startColumn: startCol,
        endLineNumber: startLine,
        endColumn: startCol + Math.max(f.field.length, 1),
        message: `[${f.rule_id}] ${f.message}${
          f.rationale ? `\n\n${f.rationale}` : ""
        }`,
        source: "aegis validate",
      };
    });
    monaco.editor.setModelMarkers(model, VALIDATE_MARKER_OWNER, markers);
  }, [validateState]);

  function handleTemplateSelect(t: Template) {
    setYaml(t.yaml);
    setActiveTemplateId(t.id);
  }

  async function handleSave() {
    setSaving(true);
    try {
      const r = await fetch("/api/v1/manifests", {
        method: "POST",
        headers: { "Content-Type": "application/x-yaml" },
        body: yaml,
      });
      if (!r.ok) {
        const text = await r.text().catch(() => `HTTP ${r.status}`);
        throw new Error(text || `HTTP ${r.status}`);
      }
      const data = (await r.json()) as SaveResponse;
      setSavedPath(data.path);
      setDirty(false);
      toast.success("Manifest saved", {
        description: `${data.bytes} bytes → ${data.path}`,
      });
    } catch (e) {
      toast.error("Save failed", {
        description: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setSaving(false);
    }
  }

  return (
    <>
      <header className="mb-8 flex items-center justify-between gap-3">
        <div className="flex items-center gap-3">
          <FileCode className="h-7 w-7 text-accent" aria-hidden="true" />
          <div>
            <h1 className="text-2xl font-semibold tracking-tight">
              Manifest Builder
            </h1>
            <p className="text-sm text-muted">
              Pick a curated starter, edit, save · live{" "}
              <code className="font-mono text-accent">aegis validate</code>{" "}
              diagnostics render as you type
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <ValidateStatus state={validateState} />
          <TemplatePicker onSelect={handleTemplateSelect} />
          <button
            type="button"
            onClick={handleSave}
            disabled={saving}
            className="inline-flex items-center gap-1.5 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-elev)] px-3 py-1.5 text-sm transition-colors hover:border-accent hover:text-accent disabled:opacity-50"
            aria-label="Save manifest"
          >
            <Save className="h-4 w-4" aria-hidden="true" />
            <span>{saving ? "Saving…" : dirty ? "Save" : "Saved"}</span>
          </button>
        </div>
      </header>

      <Card>
        <CardHeader>
          <CardTitle>
            manifest.yaml{" "}
            {activeTemplateId && (
              <span className="font-mono text-xs text-muted">
                / template:{" "}
                <span className="text-accent">{activeTemplateId}</span>
              </span>
            )}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="mb-4 text-sm text-muted">
            Monaco editor served from the embedded SPA bundle (no CDN). Each
            curated template ships with a metadata block + pain-point citation
            anchored in a documented incident (CVE, postmortem, forum
            thread); source files live at{" "}
            <a
              href="https://github.com/tosin2013/aegis-node/tree/main/examples/templates/manifests"
              target="_blank"
              rel="noopener noreferrer"
              className="font-mono text-accent underline-offset-2 hover:underline"
            >
              examples/templates/manifests/
            </a>
            . Save writes to{" "}
            <code className="font-mono text-accent">
              ~/.config/aegis/manifests/draft.yaml
            </code>
            ; load it with{" "}
            <code className="font-mono text-accent">
              aegis run --manifest …
            </code>
            .
          </p>

          {savedPath && (
            <p className="mb-4 font-mono text-xs text-muted">
              last save → <span className="text-accent">{savedPath}</span>
            </p>
          )}

          <div className="overflow-hidden rounded-md border border-[var(--color-border)]">
            <Suspense
              fallback={
                <div className="flex h-[480px] items-center justify-center font-mono text-sm text-muted">
                  loading editor…
                </div>
              }
            >
              <MonacoEditor
                height="480px"
                defaultLanguage="yaml"
                value={yaml}
                onChange={(v) => setYaml(v ?? "")}
                onMount={(editor, monaco) => {
                  editorRef.current = editor;
                  monacoRef.current = monaco;
                }}
                theme="vs-dark"
                options={{
                  minimap: { enabled: false },
                  fontSize: 13,
                  scrollBeyondLastLine: false,
                  renderWhitespace: "selection",
                }}
              />
            </Suspense>
          </div>

          <ValidateFindings state={validateState} />
        </CardContent>
      </Card>
    </>
  );
}

function monacoSeverity(
  monaco: typeof Monaco,
  severity: string,
): Monaco.MarkerSeverity {
  switch (severity) {
    case "error":
      return monaco.MarkerSeverity.Error;
    case "warn":
      return monaco.MarkerSeverity.Warning;
    case "info":
      return monaco.MarkerSeverity.Info;
    default:
      return monaco.MarkerSeverity.Hint;
  }
}

function ValidateStatus({ state }: { state: ValidateState }) {
  if (state.kind === "idle") {
    return (
      <span className="inline-flex items-center font-mono text-xs text-muted">
        validate idle
      </span>
    );
  }
  if (state.kind === "loading") {
    return (
      <span className="inline-flex items-center font-mono text-xs text-muted">
        validating…
      </span>
    );
  }
  if (state.kind === "error") {
    return (
      <span
        className="inline-flex items-center gap-1 font-mono text-xs text-danger"
        title={state.message}
      >
        <CircleAlert className="h-3.5 w-3.5" aria-hidden="true" />
        validator error
      </span>
    );
  }
  const { errors, warnings, infos } = state.summary;
  if (errors === 0 && warnings === 0 && infos === 0) {
    return (
      <span className="inline-flex items-center font-mono text-xs text-success">
        ✓ clean
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-2 font-mono text-xs">
      {errors > 0 && (
        <span className="inline-flex items-center gap-1 text-danger">
          <CircleAlert className="h-3.5 w-3.5" aria-hidden="true" />
          {errors} error{errors === 1 ? "" : "s"}
        </span>
      )}
      {warnings > 0 && (
        <span className="inline-flex items-center gap-1 text-warning">
          <TriangleAlert className="h-3.5 w-3.5" aria-hidden="true" />
          {warnings} warn{warnings === 1 ? "" : "s"}
        </span>
      )}
      {infos > 0 && (
        <span className="inline-flex items-center gap-1 text-accent">
          <Info className="h-3.5 w-3.5" aria-hidden="true" />
          {infos} info
        </span>
      )}
    </span>
  );
}

function ValidateFindings({ state }: { state: ValidateState }) {
  if (state.kind === "error") {
    return (
      <div className="mt-4 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-elev)] p-3 text-xs">
        <p className="mb-1 font-semibold text-warning">
          Live validate unavailable
        </p>
        <p className="font-mono text-[var(--color-fg)]">{state.message}</p>
        <p className="mt-2 text-muted">
          Install via{" "}
          <code className="font-mono">make build-go-validate</code>, put{" "}
          <code className="font-mono">aegis-validate</code> on PATH, or set the{" "}
          <code className="font-mono">AEGIS_VALIDATE_BIN</code> env var.
        </p>
      </div>
    );
  }
  if (state.kind !== "ready" || state.response.findings.length === 0) {
    return null;
  }
  return (
    <ul className="mt-4 space-y-2">
      {state.response.findings.map((f, i) => (
        <li
          key={`${f.rule_id}-${f.field}-${i}`}
          className="rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] p-3 text-xs"
        >
          <div className="mb-1 flex items-baseline gap-2">
            <span
              className={
                f.severity === "error"
                  ? "font-mono text-danger"
                  : f.severity === "warn"
                    ? "font-mono text-warning"
                    : "font-mono text-accent"
              }
            >
              {f.severity}
            </span>
            <span className="font-mono text-accent">{f.rule_id}</span>
            <span className="font-mono text-muted">{f.field}</span>
          </div>
          <p className="text-[var(--color-fg)]">{f.message}</p>
          {f.rationale && (
            <p className="mt-1 text-muted">{f.rationale}</p>
          )}
        </li>
      ))}
    </ul>
  );
}
