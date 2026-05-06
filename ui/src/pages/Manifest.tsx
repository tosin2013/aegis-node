import { lazy, Suspense } from "react";
import { FileCode } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

// Monaco's bundle is ~1 MB. Lazy-load it on this route so Home and
// Models stay light. Vite code-splits this dynamic import into its
// own chunk fetched only when the operator navigates to /manifest.
const MonacoEditor = lazy(async () => {
  const mod = await import("@monaco-editor/react");
  return { default: mod.default };
});

const SAMPLE_YAML = `# Sample Aegis-Node permission manifest (read-only preview).
# Sub-phase 1d.1c wires this to a real save flow + live
# \`aegis validate\` diagnostics per ADR-031 §"Visual manifest builder."

identity:
  workload: example-agent
  instance: dev-1

inference:
  determinism:
    seed: 42
    temperature: 0.0

tools:
  filesystem:
    read:
      - /home/agent/data
    write:
      - path: /home/agent/out
        ttl: PT1H
  network:
    outbound:
      - host: api.example.com
        port: 443
        protocol: tcp
  mcp:
    - server_name: fs-mcp
      server_uri: stdio:npx -y @modelcontextprotocol/server-filesystem /tmp
      allowed_tools:
        - read_text_file
`;

export function Manifest() {
  return (
    <>
      <header className="mb-8 flex items-center gap-3">
        <FileCode className="h-7 w-7 text-accent" aria-hidden="true" />
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">
            Manifest Builder
          </h1>
          <p className="text-sm text-muted">
            Preview only · live validate + save lands in sub-phase 1d.1c
          </p>
        </div>
      </header>

      <Card>
        <CardHeader>
          <CardTitle>manifest.yaml</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="mb-4 text-sm text-muted">
            Monaco editor lazy-loaded — only fetched when this route is
            visited. The full editor will integrate with{" "}
            <code className="font-mono text-accent">aegis validate</code> for
            inline diagnostics on overly broad paths, missing quotas
            (ADR-027), and redundant <code className="font-mono">pre_validate</code>{" "}
            clauses (ADR-024).
          </p>

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
                defaultValue={SAMPLE_YAML}
                theme="vs-dark"
                options={{
                  minimap: { enabled: false },
                  readOnly: true,
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
