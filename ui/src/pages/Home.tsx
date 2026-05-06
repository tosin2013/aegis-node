import { useEffect, useState } from "react";
import { ShieldCheck } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

interface VersionResponse {
  version: string;
  features: string[];
  listen: string;
}

type LoadState =
  | { kind: "loading" }
  | { kind: "ready"; data: VersionResponse }
  | { kind: "error"; message: string };

export function Home() {
  const [state, setState] = useState<LoadState>({ kind: "loading" });

  useEffect(() => {
    let cancelled = false;
    fetch("/api/v1/version")
      .then(async (r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return (await r.json()) as VersionResponse;
      })
      .then((data) => {
        if (!cancelled) setState({ kind: "ready", data });
      })
      .catch((e: unknown) => {
        if (!cancelled) {
          setState({
            kind: "error",
            message: e instanceof Error ? e.message : String(e),
          });
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <>
      <header className="mb-8 flex items-center gap-3">
        <ShieldCheck className="h-7 w-7 text-accent" aria-hidden="true" />
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Aegis-Node</h1>
          <p className="text-sm text-muted">Community UI · Phase 1d scaffold</p>
        </div>
      </header>

      <Card>
        <CardHeader>
          <CardTitle>Runtime</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="mb-5 text-sm text-muted">
            The full UI ships across v0.9.5 (Phase 1d). This page proves the
            embedded Vite + React + Tailwind pipeline works — the chat,
            trajectory, manifest builder, and model library replace this
            placeholder in subsequent sub-phases.
          </p>

          {state.kind === "loading" && (
            <p className="font-mono text-sm text-muted">loading runtime info…</p>
          )}

          {state.kind === "error" && (
            <p className="font-mono text-sm text-red-400">
              error: {state.message}
            </p>
          )}

          {state.kind === "ready" && (
            <dl className="grid grid-cols-[max-content_1fr] gap-x-5 gap-y-2 text-sm">
              <dt className="text-muted">Version</dt>
              <dd className="font-mono text-accent">{state.data.version}</dd>
              <dt className="text-muted">Features</dt>
              <dd className="font-mono text-accent">
                {state.data.features.length > 0
                  ? state.data.features.join(", ")
                  : "(none)"}
              </dd>
              <dt className="text-muted">Listen</dt>
              <dd className="font-mono text-accent">{state.data.listen}</dd>
            </dl>
          )}
        </CardContent>
      </Card>
    </>
  );
}
