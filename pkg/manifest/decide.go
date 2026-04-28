package manifest

import (
	"fmt"
	"strings"
)

// DecisionKind mirrors aegis_policy::Decision in Rust. The cross-language
// conformance harness (issue #16) asserts both engines produce the same
// kind for every query in tests/conformance/cases.json.
type DecisionKind string

const (
	DecisionAllow           DecisionKind = "allow"
	DecisionDeny            DecisionKind = "deny"
	DecisionRequireApproval DecisionKind = "require_approval"
)

// Decision is the answer to a single permission check.
type Decision struct {
	Kind   DecisionKind
	Reason string
}

func allowDecision() Decision        { return Decision{Kind: DecisionAllow} }
func denyDecision(r string) Decision { return Decision{Kind: DecisionDeny, Reason: r} }
func approvalDecision(r string) Decision {
	return Decision{Kind: DecisionRequireApproval, Reason: r}
}

// QueryKind enumerates the operation kinds the conformance harness
// exercises. Mirrors Rust's check_filesystem_* / check_network_* / check_exec.
type QueryKind string

const (
	QueryFilesystemRead   QueryKind = "filesystem_read"
	QueryFilesystemWrite  QueryKind = "filesystem_write"
	QueryFilesystemDelete QueryKind = "filesystem_delete"
	QueryNetworkOutbound  QueryKind = "network_outbound"
	QueryNetworkInbound   QueryKind = "network_inbound"
	QueryExec             QueryKind = "exec"
)

// Query is one I/O attempt described abstractly so both Go and Rust
// engines can evaluate it without invoking the underlying syscall.
type Query struct {
	Kind        QueryKind `json:"kind"`
	ResourceURI string    `json:"resource_uri,omitempty"`
	Host        string    `json:"host,omitempty"`
	Port        int       `json:"port,omitempty"`
	Protocol    string    `json:"protocol,omitempty"`
}

// Decide answers a Query against `m`. Closed-by-default: silence is
// denial. approval_required_for upgrades Allow → RequireApproval.
//
// Semantics MUST match aegis_policy::Policy::check_* in Rust. The
// conformance harness asserts agreement on every example manifest.
func (m *Manifest) Decide(q Query) Decision {
	switch q.Kind {
	case QueryFilesystemRead:
		return m.decideFsRead(q.ResourceURI)
	case QueryFilesystemWrite:
		return m.decideFsWrite(q.ResourceURI)
	case QueryFilesystemDelete:
		return m.decideFsDelete(q.ResourceURI)
	case QueryNetworkOutbound:
		return m.decideNetwork(true, q.Host, q.Port, q.Protocol)
	case QueryNetworkInbound:
		return m.decideNetwork(false, q.Host, q.Port, q.Protocol)
	case QueryExec:
		return denyDecision("exec is not grantable in manifest schema v1; future schema will add exec_grants")
	default:
		return denyDecision(fmt.Sprintf("unknown query kind %q", q.Kind))
	}
}

func (m *Manifest) decideFsRead(uri string) Decision {
	var paths []string
	if m.Tools.Filesystem != nil {
		paths = m.Tools.Filesystem.Read
	}
	if !pathCovered(uri, paths) {
		return denyDecision(fmt.Sprintf("filesystem read of %s not granted by manifest", uri))
	}
	return allowDecision()
}

func (m *Manifest) decideFsWrite(uri string) Decision {
	if g := m.findWriteGrantFor(uri, ActionWrite); g != nil {
		return m.writeGrantDecision(uri, g, ActionWrite)
	}
	var paths []string
	if m.Tools.Filesystem != nil {
		paths = m.Tools.Filesystem.Write
	}
	if pathCovered(uri, paths) {
		return upgradeForApproval(allowDecision(), m.ApprovalRequiredFor, ApprovalAnyWrite,
			"any_write requires approval")
	}
	return denyDecision(fmt.Sprintf("filesystem write of %s not granted by manifest", uri))
}

func (m *Manifest) decideFsDelete(uri string) Decision {
	if g := m.findWriteGrantFor(uri, ActionDelete); g != nil {
		return m.writeGrantDecision(uri, g, ActionDelete)
	}
	return denyDecision(fmt.Sprintf("filesystem delete of %s not granted by any write_grant", uri))
}

func (m *Manifest) decideNetwork(outbound bool, host string, port int, protocol string) Decision {
	var policy *NetworkPolicy
	if m.Tools.Network != nil {
		if outbound {
			policy = m.Tools.Network.Outbound
		} else {
			policy = m.Tools.Network.Inbound
		}
	}
	dir := "outbound"
	if !outbound {
		dir = "inbound"
	}
	base := evalNetwork(policy, host, port, protocol, dir)
	if !outbound {
		return base
	}
	return upgradeForApproval(base, m.ApprovalRequiredFor, ApprovalAnyNetworkOutbound,
		"any_network_outbound requires approval")
}

func evalNetwork(p *NetworkPolicy, host string, port int, protocol, direction string) Decision {
	if p == nil {
		return denyDecision(fmt.Sprintf("network %s denied: manifest has no policy", direction))
	}
	if p.Mode == NetworkAllow {
		return allowDecision()
	}
	if p.Mode == NetworkDeny || p.Mode == "" {
		return denyDecision(fmt.Sprintf("network %s denied: manifest sets deny", direction))
	}
	for _, e := range p.Allowlist {
		if matchesAllowEntry(e, host, port, protocol) {
			return allowDecision()
		}
	}
	return denyDecision(fmt.Sprintf("network %s %s:%d not in manifest allowlist", direction, host, port))
}

func matchesAllowEntry(e NetworkAllowEntry, host string, port int, protocol string) bool {
	if e.Host != host {
		return false
	}
	if e.Port != 0 && e.Port != port {
		return false
	}
	if e.Protocol != "" && e.Protocol != protocol {
		return false
	}
	return true
}

// pathCovered: child path is at-or-under any parent. "/data" covers
// "/data" and "/data/x" but not "/data2". MUST match Rust's
// `paths_cover` exactly.
func pathCovered(child string, parents []string) bool {
	for _, p := range parents {
		if p == child {
			return true
		}
		if p == "/" {
			return true
		}
		if strings.HasPrefix(child, p+"/") {
			return true
		}
	}
	return false
}

// Action-name shortcuts so callers don't import the WriteAction enum
// just to look up a grant.
const (
	ActionWrite  = WriteAction("write")
	ActionDelete = WriteAction("delete")
	ActionUpdate = WriteAction("update")
	ActionCreate = WriteAction("create")
)

// WriteAction string is what the YAML emits ("write" / "delete" / …).
// Defined here as a typed string so existing Manifest.WriteGrant.Actions
// interop without changing types.go signatures.
type WriteAction string

func (m *Manifest) findWriteGrantFor(uri string, action WriteAction) *WriteGrant {
	for i := range m.WriteGrants {
		g := &m.WriteGrants[i]
		if g.Resource != uri {
			continue
		}
		for _, a := range g.Actions {
			if a == string(action) {
				return g
			}
		}
	}
	return nil
}

func (m *Manifest) writeGrantDecision(uri string, g *WriteGrant, action WriteAction) Decision {
	cls := ApprovalAnyWrite
	if action == ActionDelete {
		cls = ApprovalAnyDelete
	}
	if g.ApprovalRequired || hasApprovalClass(m.ApprovalRequiredFor, cls) {
		return approvalDecision(fmt.Sprintf("%s on %s requires approval per write_grant", action, uri))
	}
	return allowDecision()
}

func upgradeForApproval(base Decision, classes []ApprovalClass, want ApprovalClass, reason string) Decision {
	if base.Kind == DecisionAllow && hasApprovalClass(classes, want) {
		return approvalDecision(reason)
	}
	return base
}

func hasApprovalClass(classes []ApprovalClass, want ApprovalClass) bool {
	for _, c := range classes {
		if c == want {
			return true
		}
	}
	return false
}
