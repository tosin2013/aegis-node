import { useQuery } from "@tanstack/react-query";
import { Boxes, Hash, ShieldCheck } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

interface ModelCosign {
  verified: boolean;
  mode: string;
  keyless_identity_pattern?: string;
  keyless_oidc_issuer_pattern?: string;
  key_path?: string;
}

interface Model {
  digest: string;
  oci_ref: string | null;
  cosign?: ModelCosign;
  size_bytes: number;
  last_used: string | null;
  pulled_at?: string;
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

function shortRef(ref: string): string {
  // Trim the @sha256:<64> tail to just the digest's first 8 chars
  // for compactness; the full ref is still in the title attribute.
  const idx = ref.indexOf("@sha256:");
  if (idx === -1) return ref;
  return `${ref.slice(0, idx)}@sha256:${ref.slice(idx + 8, idx + 16)}…`;
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
        <p className="font-mono text-sm text-danger">
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
                      {m.cosign?.verified && (
                        <CosignBadge cosign={m.cosign} />
                      )}
                    </CardTitle>
                  </CardHeader>
                  <CardContent>
                    <dl className="grid grid-cols-[max-content_1fr] gap-x-5 gap-y-1.5 text-sm">
                      {m.oci_ref && (
                        <>
                          <dt className="text-muted">OCI ref</dt>
                          <dd
                            className="truncate font-mono text-accent"
                            title={m.oci_ref}
                          >
                            {shortRef(m.oci_ref)}
                          </dd>
                        </>
                      )}
                      <dt className="text-muted">Size</dt>
                      <dd className="font-mono">{formatBytes(m.size_bytes)}</dd>
                      <dt className="text-muted">Chat template</dt>
                      <dd className="font-mono">
                        {m.has_chat_template ? "✓ present" : "—"}
                      </dd>
                      {m.cosign && (
                        <>
                          <dt className="text-muted">Cosign</dt>
                          <dd className="font-mono">
                            <CosignDetails cosign={m.cosign} />
                          </dd>
                        </>
                      )}
                      {m.pulled_at && (
                        <>
                          <dt className="text-muted">Pulled at</dt>
                          <dd className="font-mono">
                            {new Date(m.pulled_at).toLocaleString()}
                          </dd>
                        </>
                      )}
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

function CosignBadge({ cosign }: { cosign: ModelCosign }) {
  const tooltip =
    cosign.mode === "keyless"
      ? `verified · keyless · identity ${cosign.keyless_identity_pattern ?? "*"}`
      : `verified · key ${cosign.key_path ?? "*"}`;
  return (
    <span
      className="inline-flex items-center gap-1 rounded border border-[var(--color-border)] bg-[var(--color-bg-elev)] px-1.5 py-0.5 font-mono text-[10px] text-success"
      title={tooltip}
    >
      <ShieldCheck className="h-3 w-3" aria-hidden="true" />
      verified
    </span>
  );
}

function CosignDetails({ cosign }: { cosign: ModelCosign }) {
  if (cosign.mode === "key") {
    return (
      <span>
        <span className="text-success">✓ keyed</span> ·{" "}
        <span className="text-muted">key:</span>{" "}
        {cosign.key_path ?? "(unknown)"}
      </span>
    );
  }
  return (
    <span>
      <span className="text-success">✓ keyless</span>
      {cosign.keyless_identity_pattern && (
        <>
          {" "}
          · <span className="text-muted">identity:</span>{" "}
          <span className="break-all">{cosign.keyless_identity_pattern}</span>
        </>
      )}
    </span>
  );
}
