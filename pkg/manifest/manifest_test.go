package manifest

import (
	"bytes"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestEmbeddedSchemaMatchesCanonical(t *testing.T) {
	canonical, err := os.ReadFile("../../schemas/manifest/v1/manifest.schema.json")
	if err != nil {
		t.Fatalf("read canonical schema: %v", err)
	}
	if !bytes.Equal(canonical, SchemaBytes()) {
		t.Fatalf("embedded schema drift; rerun: cp schemas/manifest/v1/manifest.schema.json pkg/manifest/schema_v1.json")
	}
}

func TestLoadReadOnlyResearchExample(t *testing.T) {
	m, err := Load("../../schemas/manifest/v1/examples/read-only-research.manifest.yaml")
	if err != nil {
		t.Fatalf("load example: %v", err)
	}
	if m.SchemaVersion != "1" {
		t.Errorf("schemaVersion: got %q want 1", m.SchemaVersion)
	}
	if m.Agent.Name != "research-assistant" {
		t.Errorf("agent.name: got %q", m.Agent.Name)
	}
	if m.Tools.Filesystem == nil ||
		len(m.Tools.Filesystem.Read) != 2 ||
		m.Tools.Filesystem.Read[0] != "/data/reports" {
		t.Errorf("filesystem.read: %+v", m.Tools.Filesystem)
	}
	if m.Tools.Network == nil ||
		m.Tools.Network.Outbound == nil ||
		m.Tools.Network.Outbound.Mode != NetworkDeny {
		t.Errorf("network.outbound: %+v", m.Tools.Network)
	}
}

func TestLoadSingleWriteTargetExample(t *testing.T) {
	m, err := Load("../../schemas/manifest/v1/examples/single-write-target.manifest.yaml")
	if err != nil {
		t.Fatalf("load example: %v", err)
	}
	if len(m.WriteGrants) != 1 {
		t.Fatalf("write_grants: got %d want 1", len(m.WriteGrants))
	}
	wg := m.WriteGrants[0]
	if wg.Resource != "/data/output/daily-report.md" {
		t.Errorf("resource: %q", wg.Resource)
	}
	if !wg.ApprovalRequired {
		t.Error("approval_required should be true")
	}
	if len(m.ApprovalRequiredFor) != 2 {
		t.Errorf("approval_required_for: %v", m.ApprovalRequiredFor)
	}
}

func TestRejectsUnknownTopLevelKey(t *testing.T) {
	yaml := `schemaVersion: "1"
agent:
  name: "x"
  version: "1.0.0"
identity:
  spiffeId: "spiffe://td/agent/x/1"
tools: {}
unexpected: "value"
`
	_, err := Parse("/in-memory.yaml", []byte(yaml))
	if err == nil {
		t.Fatal("expected error, got nil")
	}
	pe := mustParseError(t, err)
	if pe.Line == 0 {
		t.Error("expected non-zero line")
	}
	if !strings.Contains(pe.Error(), "unexpected") &&
		!strings.Contains(pe.Error(), "additional") {
		t.Errorf("error should mention the offending field: %q", pe.Error())
	}
}

func TestRejectsMissingRequiredField(t *testing.T) {
	yaml := `schemaVersion: "1"
agent:
  name: "x"
  version: "1.0.0"
tools: {}
`
	_, err := Parse("m.yaml", []byte(yaml))
	if err == nil {
		t.Fatal("expected error for missing identity")
	}
	pe := mustParseError(t, err)
	if !strings.Contains(pe.Error(), "identity") &&
		!strings.Contains(pe.Error(), "required") {
		t.Errorf("error should mention identity/required: %q", pe.Error())
	}
}

func TestRejectsBadSpiffeId(t *testing.T) {
	yaml := `schemaVersion: "1"
agent:
  name: "x"
  version: "1.0.0"
identity:
  spiffeId: "not-a-spiffe-id"
tools: {}
`
	_, err := Parse("m.yaml", []byte(yaml))
	if err == nil {
		t.Fatal("expected error")
	}
	pe := mustParseError(t, err)
	if pe.Field != "/identity/spiffeId" {
		t.Errorf("field: got %q want /identity/spiffeId", pe.Field)
	}
}

func TestNetworkAllowlistShape(t *testing.T) {
	yaml := `schemaVersion: "1"
agent:
  name: "x"
  version: "1.0.0"
identity:
  spiffeId: "spiffe://td/agent/x/1"
tools:
  network:
    outbound:
      allowlist:
        - host: "api.example.com"
          port: 443
          protocol: "https"
`
	m, err := Parse("m.yaml", []byte(yaml))
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if m.Tools.Network.Outbound.Mode != NetworkAllowlist {
		t.Fatalf("mode: %q", m.Tools.Network.Outbound.Mode)
	}
	if len(m.Tools.Network.Outbound.Allowlist) != 1 {
		t.Fatalf("allowlist: %+v", m.Tools.Network.Outbound)
	}
}

func TestExtendsNarrowingAccepted(t *testing.T) {
	dir := t.TempDir()
	parent := filepath.Join(dir, "parent.yaml")
	child := filepath.Join(dir, "child.yaml")
	mustWrite(t, parent, `schemaVersion: "1"
agent: { name: "p", version: "1.0.0" }
identity: { spiffeId: "spiffe://td/agent/p/1" }
tools:
  filesystem:
    read: ["/data"]
    write: []
  network:
    outbound: deny
write_grants: []
`)
	mustWrite(t, child, `schemaVersion: "1"
agent: { name: "c", version: "1.0.0" }
identity: { spiffeId: "spiffe://td/agent/c/1" }
extends: ["parent.yaml"]
tools:
  filesystem:
    read: ["/data/reports"]
    write: []
  network:
    outbound: deny
write_grants: []
`)
	r, err := LoadResolved(child)
	if err != nil {
		t.Fatalf("LoadResolved: %v", err)
	}
	if len(r.Parents) != 1 {
		t.Errorf("parents: %d", len(r.Parents))
	}
}

func TestExtendsNarrowingRejected_FsRead(t *testing.T) {
	dir := t.TempDir()
	parent := filepath.Join(dir, "parent.yaml")
	child := filepath.Join(dir, "child.yaml")
	mustWrite(t, parent, `schemaVersion: "1"
agent: { name: "p", version: "1.0.0" }
identity: { spiffeId: "spiffe://td/agent/p/1" }
tools:
  filesystem:
    read: ["/data/reports"]
    write: []
write_grants: []
`)
	// Child tries to read /etc — not under any parent path.
	mustWrite(t, child, `schemaVersion: "1"
agent: { name: "c", version: "1.0.0" }
identity: { spiffeId: "spiffe://td/agent/c/1" }
extends: ["parent.yaml"]
tools:
  filesystem:
    read: ["/etc"]
    write: []
write_grants: []
`)
	_, err := LoadResolved(child)
	if err == nil {
		t.Fatal("expected NarrowingError")
	}
	var ne *NarrowingError
	if !errors.As(err, &ne) {
		t.Fatalf("error type: %T %v", err, err)
	}
	if ne.Field != "tools.filesystem.read" {
		t.Errorf("field: %q", ne.Field)
	}
}

func TestExtendsNarrowingRejected_DropApprovalClass(t *testing.T) {
	dir := t.TempDir()
	parent := filepath.Join(dir, "parent.yaml")
	child := filepath.Join(dir, "child.yaml")
	// Parent insists any_write needs approval; child omits it.
	mustWrite(t, parent, `schemaVersion: "1"
agent: { name: "p", version: "1.0.0" }
identity: { spiffeId: "spiffe://td/agent/p/1" }
tools: {}
approval_required_for: ["any_write"]
`)
	mustWrite(t, child, `schemaVersion: "1"
agent: { name: "c", version: "1.0.0" }
identity: { spiffeId: "spiffe://td/agent/c/1" }
extends: ["parent.yaml"]
tools: {}
approval_required_for: []
`)
	_, err := LoadResolved(child)
	if err == nil {
		t.Fatal("expected NarrowingError")
	}
	var ne *NarrowingError
	if !errors.As(err, &ne) {
		t.Fatalf("error type: %T", err)
	}
	if ne.Field != "approval_required_for" {
		t.Errorf("field: %q", ne.Field)
	}
}

func TestExtendsCycleDetected(t *testing.T) {
	dir := t.TempDir()
	a := filepath.Join(dir, "a.yaml")
	b := filepath.Join(dir, "b.yaml")
	mustWrite(t, a, `schemaVersion: "1"
agent: { name: "a", version: "1.0.0" }
identity: { spiffeId: "spiffe://td/agent/a/1" }
extends: ["b.yaml"]
tools: {}
`)
	mustWrite(t, b, `schemaVersion: "1"
agent: { name: "b", version: "1.0.0" }
identity: { spiffeId: "spiffe://td/agent/b/1" }
extends: ["a.yaml"]
tools: {}
`)
	_, err := LoadResolved(a)
	if err == nil {
		t.Fatal("expected CycleError")
	}
	var ce *CycleError
	if !errors.As(err, &ce) {
		t.Fatalf("error type: %T %v", err, err)
	}
}

func TestExecGrantsNarrowingRejected(t *testing.T) {
	dir := t.TempDir()
	parent := filepath.Join(dir, "parent.yaml")
	child := filepath.Join(dir, "child.yaml")
	mustWrite(t, parent, `schemaVersion: "1"
agent: { name: "p", version: "1.0.0" }
identity: { spiffeId: "spiffe://td/agent/p/1" }
tools: {}
exec_grants:
  - program: "/usr/bin/git"
`)
	mustWrite(t, child, `schemaVersion: "1"
agent: { name: "c", version: "1.0.0" }
identity: { spiffeId: "spiffe://td/agent/c/1" }
extends: ["parent.yaml"]
tools: {}
exec_grants:
  - program: "/usr/bin/curl"
`)
	_, err := LoadResolved(child)
	if err == nil {
		t.Fatal("expected NarrowingError for new exec program")
	}
	var ne *NarrowingError
	if !errors.As(err, &ne) {
		t.Fatalf("error type: %T", err)
	}
	if !strings.HasPrefix(ne.Field, "exec_grants[") {
		t.Errorf("field: %q", ne.Field)
	}
}

func mustWrite(t *testing.T, path, body string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatalf("write %s: %v", path, err)
	}
}

func mustParseError(t *testing.T, err error) *ParseError {
	t.Helper()
	var pe *ParseError
	if !errors.As(err, &pe) {
		t.Fatalf("expected *ParseError, got %T: %v", err, err)
	}
	return pe
}
