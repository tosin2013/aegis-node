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

// Per ADR-018 / issue #46. Loads the agent-with-mcp example manifest
// (research agent + Anthropic filesystem MCP server, read-only subset).
// Catches drift if the example is hand-edited into a malformed shape.
func TestLoadAgentWithMCPExample(t *testing.T) {
	m, err := Load("../../schemas/manifest/v1/examples/agent-with-mcp.manifest.yaml")
	if err != nil {
		t.Fatalf("load example: %v", err)
	}
	if len(m.Tools.MCP) != 1 {
		t.Fatalf("tools.mcp: got %d entries, want 1", len(m.Tools.MCP))
	}
	server := m.Tools.MCP[0]
	if server.ServerName != "filesystem" {
		t.Errorf("server_name: %q", server.ServerName)
	}
	if server.ServerURI != "stdio:/usr/local/bin/mcp-server-filesystem" {
		t.Errorf("server_uri: %q", server.ServerURI)
	}
	if len(server.AllowedTools) == 0 {
		t.Fatal("allowed_tools must be non-empty for this fixture")
	}
	// Read-only invariant: none of the listed tools may be a writer.
	writers := map[string]bool{
		"write_file":       true,
		"edit_file":        true,
		"move_file":        true,
		"create_directory": true,
	}
	for _, tool := range server.AllowedTools {
		if writers[tool] {
			t.Errorf("agent-with-mcp must stay read-only; %q is a writer", tool)
		}
	}

	// Decide() agreement with the new tools.mcp[] semantics.
	allowed := m.Decide(Query{
		Kind:      QueryMCPToolCall,
		MCPServer: "filesystem",
		MCPTool:   "read_text_file",
	})
	if allowed.Kind != DecisionAllow {
		t.Errorf("read_text_file should be allowed: got %q", allowed.Kind)
	}
	denied := m.Decide(Query{
		Kind:      QueryMCPToolCall,
		MCPServer: "filesystem",
		MCPTool:   "write_file",
	})
	if denied.Kind != DecisionDeny {
		t.Errorf("write_file should be denied: got %q", denied.Kind)
	}
}

// Per ADR-018 / issue #43. Parses a valid `tools.mcp[]` example.
func TestMCPServerGrantParses(t *testing.T) {
	yaml := `schemaVersion: "1"
agent:
  name: "x"
  version: "1.0.0"
identity:
  spiffeId: "spiffe://td/agent/x/1"
tools:
  mcp:
    - server_name: "fs-helper"
      server_uri: "stdio:/usr/local/bin/mcp-fs"
      allowed_tools: ["read_file", "list_dir"]
    - server_name: "web-search"
      server_uri: "https://mcp.example.com:8443"
      allowed_tools: []
`
	m, err := Parse("m.yaml", []byte(yaml))
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if len(m.Tools.MCP) != 2 {
		t.Fatalf("mcp: got %d entries, want 2", len(m.Tools.MCP))
	}
	if m.Tools.MCP[0].ServerName != "fs-helper" ||
		m.Tools.MCP[0].ServerURI != "stdio:/usr/local/bin/mcp-fs" ||
		len(m.Tools.MCP[0].AllowedTools) != 2 ||
		m.Tools.MCP[0].AllowedTools[0] != "read_file" {
		t.Errorf("mcp[0]: %+v", m.Tools.MCP[0])
	}
	if m.Tools.MCP[1].ServerName != "web-search" ||
		len(m.Tools.MCP[1].AllowedTools) != 0 {
		t.Errorf("mcp[1]: %+v", m.Tools.MCP[1])
	}
}

// Per ADR-018 / issue #43. A malformed entry (missing required server_uri)
// must be rejected by the schema.
func TestMCPServerGrantRejectsMissingURI(t *testing.T) {
	yaml := `schemaVersion: "1"
agent:
  name: "x"
  version: "1.0.0"
identity:
  spiffeId: "spiffe://td/agent/x/1"
tools:
  mcp:
    - server_name: "fs-helper"
      allowed_tools: ["read_file"]
`
	_, err := Parse("m.yaml", []byte(yaml))
	if err == nil {
		t.Fatal("expected error for missing server_uri")
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
