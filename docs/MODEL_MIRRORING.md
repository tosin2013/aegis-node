# Mirroring an Upstream Model — Operator Workflow

This is the practical "how to publish a model your runtime can pull"
doc. It walks an operator through the same pipeline the Aegis-Node
project itself uses to publish demo models, scaled for an org's
internal registry, scanning policy, and cosign trust root.

**Per**:
- [ADR-013](adrs/013-oci-artifacts-for-model-distribution.md) — models
  ship as signed OCI artifacts.
- [ADR-021](adrs/021-huggingface-as-upstream-oci-as-trust-boundary.md)
  — HuggingFace is the canonical upstream; OCI + cosign is the trust
  boundary the runtime sees.
- [`.github/workflows/models-publish.yml`](../.github/workflows/models-publish.yml)
  — the project's reference implementation; this doc adapts it.
- OCI-C tracking issue: [#68](https://github.com/tosin2013/aegis-node/issues/68).

## TL;DR

```
HuggingFace (upstream)
   │  hf download <repo> <file> --revision <commit-sha>
   ▼
Operator workstation / CI runner
   │  scan (AV + injection corpus + license review)
   │  oras push  → <internal-registry>/<repo>@sha256:<manifest-digest>
   │  cosign sign (your org's keyless OIDC or KMS key)
   ▼
Internal registry (Harbor / Artifactory / ECR / Quay)
   │  aegis pull <ref>@sha256:<digest> --keyless-identity <org regex>
   ▼
Air-gapped session boot
```

Five steps. Every gate is auditable by a security reviewer with
standard tooling (`hf`, `oras`, `cosign`, `sha256sum`).

## Step 1: Pick the upstream and pin a commit SHA

Operators in production should pin **HuggingFace commit SHAs**, not
branch/tag refs — branches move. The published Qwen artifact is
pinned to `91cad51170dc346986eccefdc2dd33a9da36ead9`.

```bash
# Resolve the current head SHA of the model repo's main branch.
curl -s "https://huggingface.co/api/models/<owner>/<repo>" \
  | jq -r '.sha'
```

Snapshot that SHA in your operator changelog so a security reviewer
can correlate the artifact you publish with the exact upstream bytes.

## Step 2: Download (commit-pinned)

```bash
hf download \
  --revision <40-char-commit-sha> \
  --local-dir ./artifact \
  <owner>/<repo> \
  <filename>.gguf
```

Why `hf` (not `huggingface-cli`): `huggingface-cli` was deprecated in
the `huggingface_hub >= 0.32.0` rename. `hf` ships with the same
package; semantics are identical.

## Step 3: Scan and license-review

This is the gate the project's `models-publish.yml` does **not**
exercise — orgs own their scanning policy. Standard checklist:

- **Malware AV** — ClamAV, your enterprise scanner, your CI scanner.
- **Prompt-injection corpus** — run a battery of known jailbreak /
  injection prompts against the model and record the violation rate.
  No fixed pass/fail bar; document the result so the security review
  can decide.
- **License review** — confirm the upstream's license permits your
  intended use (re-distribution to internal users, commercial,
  air-gap, etc.). Apache 2.0 / MIT / similarly-permissive is the
  no-legal-review path; Llama 3 / Llama 4 / vendor-restricted models
  need legal sign-off per your org policy.
- **Provenance check** — cross-reference the upstream commit SHA
  against the upstream's own release notes / model card. Reject if
  the SHA is force-pushed / rewritten.

Record everything in your changelog; the changelog itself becomes an
audit artifact.

## Step 4: Push as an OCI artifact

The artifact layout the project uses (single-blob GGUF with a custom
artifact-type and traceability annotations):

```bash
cd ./artifact
oras push <internal-registry>/<repo>:<tag> \
  --artifact-type "application/vnd.aegis-node.model.gguf.v1" \
  --annotation "org.opencontainers.image.source=https://huggingface.co/<owner>/<repo>" \
  --annotation "org.opencontainers.image.revision=<commit-sha>" \
  --annotation "org.opencontainers.image.title=<filename>.gguf" \
  --annotation "dev.aegis-node.upstream=huggingface" \
  --annotation "dev.aegis-node.upstream.repo=<owner>/<repo>" \
  --annotation "dev.aegis-node.upstream.revision=<commit-sha>" \
  --annotation "dev.aegis-node.upstream.filename=<filename>.gguf" \
  "<filename>.gguf:application/vnd.aegis-node.model.gguf.v1"
```

The annotations are advisory but powerful: any tool inspecting the
manifest can trace the artifact back to its HF upstream. Add your
own org-specific keys (e.g. `<corp>.security.scan.report-url`,
`<corp>.licensing.legal-ticket`) so future audits can find the
evidence inline.

`cd` into the artifact directory before `oras push` — `oras` rejects
absolute paths in the layer-name component to keep the manifest
portable across registries.

## Step 5: Sign with cosign

Two acceptable flows. Pick the one that matches your org's signing
identity.

### 5a. Sigstore keyless (recommended for non-air-gapped)

Same flow ADR-017 establishes for the project's devbox image. Tie the
signature to the workflow's GitHub OIDC identity (or your CI's
equivalent OIDC provider):

```bash
COSIGN_YES=true cosign sign <internal-registry>/<repo>@<manifest-digest>
```

Verifiers consume:

```bash
cosign verify <internal-registry>/<repo>@<manifest-digest> \
  --certificate-identity-regexp '<org-specific identity regex>' \
  --certificate-oidc-issuer '<your OIDC issuer>'
```

Successful verification fetches the cert + Rekor entry; air-gapped
consumers should pin the Rekor entry to disk and use `cosign verify
--offline` (per ADR-017 §"Option B").

### 5b. KMS-backed key (recommended for tightly-regulated orgs)

```bash
cosign sign \
  --key 'awskms:///<arn>' \
  <internal-registry>/<repo>@<manifest-digest>
```

Verify:

```bash
cosign verify \
  --key '<public-key.pub>' \
  <internal-registry>/<repo>@<manifest-digest>
```

The public key file is an artifact your security team distributes
with their root certificate bundle.

## Step 6: Pin the resulting digest in your operator config

The runtime contract from
[ADR-013 / OCI-A / `pull::pull`](../crates/cli/src/pull.rs) is:

```bash
aegis pull <internal-registry>/<repo>@sha256:<manifest-digest> \
  --keyless-identity '<org-specific identity regex>' \
  --keyless-oidc-issuer '<your OIDC issuer>'
```

`<manifest-digest>` is what `oras manifest fetch --descriptor` returns
on push. **Tags alone are refused by `aegis pull`** — they can move,
which would invalidate the F1 SVID-binding promise. Always ship by
digest.

For air-gapped sessions, the same ref works against your internal
registry — no Sigstore network dependency once the cosign signature
is verified once and the Rekor entry is bundled locally.

## Putting it together: Aegis-Node's own pipeline as a worked example

The project's [`models-publish.yml`](../.github/workflows/models-publish.yml)
runs steps 2–5 unattended on `workflow_dispatch`. Inputs:

```bash
gh workflow run models-publish.yml --ref main \
  -f hf_repo=Qwen/Qwen2.5-1.5B-Instruct-GGUF \
  -f gguf_filename=qwen2.5-1.5b-instruct-q4_k_m.gguf \
  -f hf_revision=91cad51170dc346986eccefdc2dd33a9da36ead9
```

Output (from [run 25135210278](https://github.com/tosin2013/aegis-node/actions/runs/25135210278)):

```
ref:                ghcr.io/tosin2013/aegis-node-models/qwen2.5-1.5b-instruct-q4_k_m:latest
manifest_digest:    sha256:240ece322070801d583241caaeced1a6b1ac55cbe42bf5379e95735ca89d4fa6
blob_sha256:        6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e
```

Pulled at runtime via:

```bash
aegis pull \
  ghcr.io/tosin2013/aegis-node-models/qwen2.5-1.5b-instruct-q4_k_m@sha256:240ece322070801d583241caaeced1a6b1ac55cbe42bf5379e95735ca89d4fa6 \
  --keyless-identity '^https://github\.com/tosin2013/aegis-node/\.github/workflows/models-publish\.yml@.*$' \
  --keyless-oidc-issuer 'https://token.actions.githubusercontent.com'
```

The exact same flow runs from
[`crates/cli/tests/pull_real_image.rs`](../crates/cli/tests/pull_real_image.rs)
on every PR — so this doc isn't aspirational; the bytes you see in
production are the bytes CI verified.

## Adapting `models-publish.yml` to your org

Fork the workflow into your operator repo, then make four changes:

1. **Replace the registry hostname** in the `target_repo` default:
   `ghcr.io/<your-org>/aegis-node-models/<model>`. Or pass
   `target_repo` explicitly per dispatch.
2. **Add a scanning step before `oras push`** — Step 3 above.
   Suggested placement is after `Compute and record SHA-256` and
   before `Push as single-blob OCI artifact`. Fail-closed: any
   scanner non-zero exit aborts the push.
3. **Adjust `permissions:`** — keyless signing needs `id-token:
   write`; pushing to your internal registry needs whatever auth
   token your registry accepts (often a deploy-token in `secrets`,
   not the workflow's `GITHUB_TOKEN`).
4. **Update the cosign verify identity regex** in the final step
   to match your org's workflow identity. The regex must match the
   Sigstore Fulcio cert that signing produces; mis-set, the
   verify-the-signature-we-just-produced gate will fail loudly
   (which is the correct behavior).

## License gate — what the project mirrors and what it doesn't

Per ADR-021 §"License scope", the Aegis-Node project's published
mirror covers Apache 2.0 / MIT / similarly-permissive models only.
The project does **not** publish:

- Llama 3.x / Llama 4.x — vendor-license restrictions.
- Vendor-tier-locked models (cohere, anthropic, openai) — those are
  remote APIs, not GGUFs.
- Models with research-only / non-commercial license terms.

Operators are free to publish those to their own internal registry
under their org's legal review. The mirror pipeline doesn't care
about license; the *publishing decision* is the gate the project
enforces upstream of running the workflow.

## What this doc does NOT cover

- **Runtime side of `aegis pull`** — already covered in
  [SUPPLY_CHAIN.md](SUPPLY_CHAIN.md) §"\`aegis pull\` (OCI-A, ADR-013)".
- **GGUF + chat-template-bound verification** — coming with OCI-B
  ([#67](https://github.com/tosin2013/aegis-node/issues/67)). Until
  it lands, a malicious actor with push access to your registry
  *and* a valid org cosign identity could substitute the chat
  template embedded in the GGUF. ADR-013 names this; OCI-B closes it.
- **Re-publishing on upstream rev bumps** — when the HF upstream
  bumps a model, you re-run the workflow with the new commit SHA.
  Both old and new digests are valid artifacts; pin the new one in
  your operator config when you've completed the scan + review.
- **Mirror retention** — old digests stay published for replay /
  audit purposes; the project doesn't garbage-collect.
