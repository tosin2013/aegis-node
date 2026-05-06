import { useQuery } from "@tanstack/react-query";
import { Boxes, Hash } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

interface Model {
  digest: string;
  oci_ref: string | null;
  size_bytes: number;
  last_used: string | null;
  has_chat_template: boolean;
}

interface ModelsResponse {
  cache_dir: string;
  models: Model[];
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
}

function shortDigest(digest: string): string {
  // sha256:<64 hex> → sha256:abcd1234…cdef
  if (digest.length <= 24) return digest;
  return `${digest.slice(0, 18)}…${digest.slice(-8)}`;
}

export function Models() {
  const { data, error, isLoading } = useQuery<ModelsResponse>({
    queryKey: ["models"],
    queryFn: async () => {
      const r = await fetch("/api/v1/models");
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      return r.json();
    },
    refetchOnWindowFocus: false,
  });

  return (
    <>
      <header className="mb-8 flex items-center gap-3">
        <Boxes className="h-7 w-7 text-accent" aria-hidden="true" />
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">
            Model Library
          </h1>
          <p className="text-sm text-muted">
            Locally cached models · pull/verify lands in sub-phase 1d.1c
          </p>
        </div>
      </header>

      {isLoading && (
        <p className="font-mono text-sm text-muted">loading model cache…</p>
      )}

      {error && (
        <p className="font-mono text-sm text-red-400">
          error: {error instanceof Error ? error.message : String(error)}
        </p>
      )}

      {data && (
        <>
          <p className="mb-4 text-xs text-muted">
            Cache:{" "}
            <span className="font-mono text-accent">{data.cache_dir}</span>
          </p>

          {data.models.length === 0 ? (
            <Card>
              <CardContent className="py-10 text-center text-sm text-muted">
                <p className="mb-2">No models cached yet.</p>
                <p>
                  Pull one from a registry via{" "}
                  <code className="font-mono text-accent">
                    aegis pull &lt;ref&gt;
                  </code>
                  . Visual pull lands in 1d.1c per ADR-032.
                </p>
              </CardContent>
            </Card>
          ) : (
            <div className="space-y-3">
              {data.models.map((m) => (
                <Card key={m.digest}>
                  <CardHeader className="pb-2">
                    <CardTitle className="flex items-center gap-2 text-sm">
                      <Hash
                        className="h-4 w-4 text-muted"
                        aria-hidden="true"
                      />
                      <span className="font-mono text-accent">
                        {shortDigest(m.digest)}
                      </span>
                    </CardTitle>
                  </CardHeader>
                  <CardContent>
                    <dl className="grid grid-cols-[max-content_1fr] gap-x-5 gap-y-1.5 text-sm">
                      {m.oci_ref && (
                        <>
                          <dt className="text-muted">OCI ref</dt>
                          <dd className="truncate font-mono text-accent">
                            {m.oci_ref}
                          </dd>
                        </>
                      )}
                      <dt className="text-muted">Size</dt>
                      <dd className="font-mono">{formatBytes(m.size_bytes)}</dd>
                      <dt className="text-muted">Chat template</dt>
                      <dd className="font-mono">
                        {m.has_chat_template ? "✓ present" : "—"}
                      </dd>
                      {m.last_used && (
                        <>
                          <dt className="text-muted">Last used</dt>
                          <dd className="font-mono">
                            {new Date(m.last_used).toLocaleString()}
                          </dd>
                        </>
                      )}
                    </dl>
                  </CardContent>
                </Card>
              ))}
            </div>
          )}
        </>
      )}
    </>
  );
}
