import { lazy, Suspense, useEffect, useState } from "react";
import { FileCode, Save } from "lucide-react";
import { toast } from "sonner";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { TemplatePicker } from "@/components/TemplatePicker";
import type { Template } from "@/templates/types";

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
# \`aegis validate\` diagnostics land in sub-phase 1d.1d.

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

export function Manifest() {
  const [yaml, setYaml] = useState<string>(FALLBACK_YAML);
  const [activeTemplateId, setActiveTemplateId] = useState<string | null>(
    null,
  );
  const [savedPath, setSavedPath] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);

  useEffect(() => {
    setDirty(yaml !== FALLBACK_YAML || savedPath !== null);
  }, [yaml, savedPath]);

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
              diagnostics land in 1d.1d
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
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
            <code className="font-mono text-accent">
              examples/templates/manifests/
            </code>
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
        </CardContent>
      </Card>
    </>
  );
}
