package validate

import (
	"fmt"
	"path"
	"strings"

	"github.com/tosin2013/aegis-node/pkg/manifest"
)

// systemRoots are paths whose blanket coverage is almost never the
// operator's intent. Listing /etc lets the agent read /etc/shadow and
// every secret token mounted as a file.
var systemRoots = []string{"/", "/etc", "/usr", "/var", "/proc", "/sys", "/dev", "/root", "/boot"}

// registry returns the canonical rule set. Order doesn't matter — Lint
// sorts findings — but stability is preserved by appending new rules to
// the end with the next AEGIS<NNN> ID.
func registry() []Rule {
	return []Rule{
		{
			ID:      "AEGIS001",
			Default: SeverityError,
			Summary: "tools.filesystem.read covers a system root (/, /etc, /usr, ...)",
			Rationale: "Blanket read of system roots gives the agent access to credentials, " +
				"tokens, and host config. Almost never the operator's intent. Narrow the path " +
				"to the data the agent actually needs (e.g. /data/reports).",
			Check: ruleFilesystemRead,
		},
		{
			ID:      "AEGIS002",
			Default: SeverityError,
			Summary: "tools.filesystem.write covers a system root (/, /etc, /usr, ...)",
			Rationale: "Blanket write of system roots is a production-mutation footgun. F7 " +
				"requires explicit write_grants for mutation; the broad rule should reach " +
				"only directories the agent owns end-to-end.",
			Check: ruleFilesystemWrite,
		},
		{
			ID:      "AEGIS003",
			Default: SeverityError,
			Summary: "exec_grant uses a bare basename without any_exec approval",
			Rationale: "A bare-basename exec_grant matches that program at any path on " +
				"$PATH — including a malicious binary an attacker dropped into a writable " +
				"directory. Either pin the absolute path, or require human approval for " +
				"every exec via approval_required_for: [any_exec].",
			Check: ruleExecBasenameWithoutApproval,
		},
		{
			ID:      "AEGIS004",
			Default: SeverityWarn,
			Summary: "tools.network.outbound: allow without any_network_outbound approval",
			Rationale: "Open egress (allow) lets the agent reach any host on any port. F6 " +
				"recommends an explicit allowlist; if open egress is required, gate every " +
				"connection through approval_required_for: [any_network_outbound].",
			Check: ruleOutboundAllowWithoutApproval,
		},
		{
			ID:      "AEGIS005",
			Default: SeverityError,
			Summary: "write_grant resource ends with / (covers a directory, not a file)",
			Rationale: "A trailing slash makes the grant cover a directory tree rather than a " +
				"specific file. F7 is precise by design — name the file. If you need many " +
				"files, list them; if you need a tree, audit explicitly via tools.filesystem.write.",
			Check: ruleWriteGrantDirectory,
		},
		{
			ID:      "AEGIS006",
			Default: SeverityWarn,
			Summary: "write_grant has no duration AND no expires_at (eternal grant)",
			Rationale: "Without a time bound, a write_grant lives for the life of every session " +
				"that loads this manifest forever. Time-bound the grant (duration: PT1H or " +
				"expires_at: 2026-12-31T00:00:00Z) so an old manifest doesn't carry indefinite " +
				"production-mutation rights.",
			Check: ruleWriteGrantEternal,
		},
		{
			ID:      "AEGIS007",
			Default: SeverityWarn,
			Summary: "tools.filesystem.write set but no any_write approval class",
			Rationale: "Manifest grants broad write coverage but never asks for human approval. " +
				"Either lift writes into explicit time-bounded write_grants, or add " +
				"approval_required_for: [any_write] so unexpected writes go through the F3 gate.",
			Check: ruleWriteWithoutApproval,
		},
		{
			ID:      "AEGIS008",
			Default: SeverityWarn,
			Summary: "tools.mcp[] entry has empty allowed_tools (no tools allowed)",
			Rationale: "Closed-by-default already denies all MCP tools when the server isn't " +
				"listed. An entry with allowed_tools: [] is functionally identical but " +
				"misleading — it suggests intent to allow tools without naming any. Either " +
				"name the tools or remove the entry.",
			Check: ruleMCPEmptyAllowedTools,
		},
		{
			ID:      "AEGIS009",
			Default: SeverityWarn,
			Summary: "approval_required_for set but approval_authorities is empty",
			Rationale: "approval_required_for triggers the F3 approval gate; the mTLS signed-API " +
				"channel requires approval_authorities to know which SPIFFE IDs may sign " +
				"approvals. Empty list = mTLS approvals will be refused. Acceptable if you " +
				"only use TTY/file/web channels — flagged so the choice is intentional.",
			Check: ruleApprovalAuthoritiesEmpty,
		},
		{
			ID:      "AEGIS010",
			Default: SeverityInfo,
			Summary: "agent.name doesn't match the workload segment of identity.spiffeId",
			Rationale: "Convention is spiffe://<trust-domain>/agent/<workload>/<instance> with " +
				"<workload> matching agent.name. Drift here is usually a copy-paste typo and " +
				"will surprise an operator reading both fields. Not enforced — the SPIFFE " +
				"path is the source of truth at runtime.",
			Check: ruleAgentNameMatchesSpiffeID,
		},
	}
}

// ---------------------------------------------------------------------------
// Rule implementations
// ---------------------------------------------------------------------------

func ruleFilesystemRead(m *manifest.Manifest) []Finding {
	if m.Tools.Filesystem == nil {
		return nil
	}
	var out []Finding
	for i, p := range m.Tools.Filesystem.Read {
		if isSystemRoot(p) {
			out = append(out, Finding{
				Field:   fmt.Sprintf("tools.filesystem.read[%d]", i),
				Message: fmt.Sprintf("path %q is a system root; narrow to the data the agent actually needs", p),
			})
		}
	}
	return out
}

func ruleFilesystemWrite(m *manifest.Manifest) []Finding {
	if m.Tools.Filesystem == nil {
		return nil
	}
	var out []Finding
	for i, p := range m.Tools.Filesystem.Write {
		if isSystemRoot(p) {
			out = append(out, Finding{
				Field:   fmt.Sprintf("tools.filesystem.write[%d]", i),
				Message: fmt.Sprintf("path %q is a system root; this is almost never intended", p),
			})
		}
	}
	return out
}

func ruleExecBasenameWithoutApproval(m *manifest.Manifest) []Finding {
	hasAnyExec := false
	for _, c := range m.ApprovalRequiredFor {
		if c == manifest.ApprovalAnyExec {
			hasAnyExec = true
			break
		}
	}
	if hasAnyExec {
		return nil
	}
	var out []Finding
	for i, g := range m.ExecGrants {
		if !strings.Contains(g.Program, "/") {
			out = append(out, Finding{
				Field: fmt.Sprintf("exec_grants[%d].program", i),
				Message: fmt.Sprintf(
					"bare basename %q matches any path on $PATH; either pin an absolute path or add approval_required_for: [any_exec]",
					g.Program,
				),
			})
		}
	}
	return out
}

func ruleOutboundAllowWithoutApproval(m *manifest.Manifest) []Finding {
	if m.Tools.Network == nil || m.Tools.Network.Outbound == nil {
		return nil
	}
	if m.Tools.Network.Outbound.Mode != manifest.NetworkAllow {
		return nil
	}
	for _, c := range m.ApprovalRequiredFor {
		if c == manifest.ApprovalAnyNetworkOutbound {
			return nil
		}
	}
	return []Finding{{
		Field: "tools.network.outbound",
		Message: "open egress (allow) is set without approval_required_for: [any_network_outbound]; " +
			"prefer an explicit allowlist or gate through the F3 approval channel",
	}}
}

func ruleWriteGrantDirectory(m *manifest.Manifest) []Finding {
	var out []Finding
	for i, g := range m.WriteGrants {
		if strings.HasSuffix(g.Resource, "/") {
			out = append(out, Finding{
				Field: fmt.Sprintf("write_grants[%d].resource", i),
				Message: fmt.Sprintf(
					"%q ends with /; F7 grants are per-file. Name the file or move the coverage to tools.filesystem.write",
					g.Resource,
				),
			})
		}
	}
	return out
}

func ruleWriteGrantEternal(m *manifest.Manifest) []Finding {
	var out []Finding
	for i, g := range m.WriteGrants {
		if g.Duration == "" && g.ExpiresAt == "" {
			out = append(out, Finding{
				Field: fmt.Sprintf("write_grants[%d]", i),
				Message: fmt.Sprintf(
					"grant on %q has no duration and no expires_at; add a time bound (e.g. duration: PT1H)",
					g.Resource,
				),
			})
		}
	}
	return out
}

func ruleWriteWithoutApproval(m *manifest.Manifest) []Finding {
	if m.Tools.Filesystem == nil || len(m.Tools.Filesystem.Write) == 0 {
		return nil
	}
	for _, c := range m.ApprovalRequiredFor {
		if c == manifest.ApprovalAnyWrite {
			return nil
		}
	}
	// Per-grant approval_required also satisfies the rule — operators may
	// have decided per-resource gating is enough.
	for _, g := range m.WriteGrants {
		if g.ApprovalRequired {
			return nil
		}
	}
	return []Finding{{
		Field: "tools.filesystem.write",
		Message: "broad write coverage set without any_write approval class or per-grant approval_required; " +
			"unexpected writes will land without human review",
	}}
}

func ruleMCPEmptyAllowedTools(m *manifest.Manifest) []Finding {
	var out []Finding
	for i, s := range m.Tools.MCP {
		if len(s.AllowedTools) == 0 {
			out = append(out, Finding{
				Field: fmt.Sprintf("tools.mcp[%d].allowed_tools", i),
				Message: fmt.Sprintf(
					"server %q lists no allowed_tools; entry is misleading — name tools or remove the entry",
					s.ServerName,
				),
			})
		}
	}
	return out
}

func ruleApprovalAuthoritiesEmpty(m *manifest.Manifest) []Finding {
	if len(m.ApprovalRequiredFor) == 0 {
		return nil
	}
	if len(m.ApprovalAuthorities) > 0 {
		return nil
	}
	return []Finding{{
		Field: "approval_authorities",
		Message: "approval_required_for is set but approval_authorities is empty; " +
			"mTLS signed-API approvals will be refused (TTY/file/web channels still work)",
	}}
}

func ruleAgentNameMatchesSpiffeID(m *manifest.Manifest) []Finding {
	// Expected: spiffe://<td>/agent/<workload>/<instance>
	id := m.Identity.SpiffeID
	if !strings.HasPrefix(id, "spiffe://") {
		return nil // schema enforces the prefix; nothing to compare
	}
	rest := strings.TrimPrefix(id, "spiffe://")
	parts := strings.Split(rest, "/")
	// parts: [<td>, "agent", "<workload>", "<instance>"]
	if len(parts) < 4 || parts[1] != "agent" {
		return nil // non-conventional layout; rule doesn't apply
	}
	workload := parts[2]
	if workload == m.Agent.Name {
		return nil
	}
	return []Finding{{
		Field: "agent.name",
		Message: fmt.Sprintf(
			"agent.name=%q doesn't match SPIFFE workload segment %q (from %q)",
			m.Agent.Name, workload, id,
		),
	}}
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// isSystemRoot returns true if p exactly equals one of the well-known
// system roots OR is a single-character absolute path. The exact-match
// check matters: /usr-local/bin shouldn't trip on /usr.
func isSystemRoot(p string) bool {
	p = path.Clean(p)
	for _, r := range systemRoots {
		if p == r {
			return true
		}
	}
	return false
}
