package manifest

import (
	"fmt"
	"path/filepath"
	"strconv"
	"strings"
	"time"
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
	// QueryMCPToolCall consults tools.mcp[] (per ADR-018 / F2-MCP-B).
	// The Query's MCPServer + MCPTool fields carry the (server, tool)
	// pair to look up.
	QueryMCPToolCall QueryKind = "mcp_tool_call"
)

// Query is one I/O attempt described abstractly so both Go and Rust
// engines can evaluate it without invoking the underlying syscall.
//
// Now and SessionStart are consulted for time-bounded write_grants
// (per F7 / issue #38). Zero values mean "no clock anchor available";
// in that case write_grants with `duration` or `expires_at` are treated
// as unbound (current behavior pre-#38 — required for back-compat with
// fixtures written before the clock fields existed).
type Query struct {
	Kind         QueryKind `json:"kind"`
	ResourceURI  string    `json:"resource_uri,omitempty"`
	Host         string    `json:"host,omitempty"`
	Port         int       `json:"port,omitempty"`
	Protocol     string    `json:"protocol,omitempty"`
	Now          time.Time `json:"now,omitempty"`
	SessionStart time.Time `json:"session_start,omitempty"`
	// MCPServer + MCPTool are consulted only for QueryMCPToolCall.
	MCPServer string `json:"mcp_server,omitempty"`
	MCPTool   string `json:"mcp_tool,omitempty"`
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
		return m.decideFsWrite(q.ResourceURI, q.Now, q.SessionStart)
	case QueryFilesystemDelete:
		return m.decideFsDelete(q.ResourceURI, q.Now, q.SessionStart)
	case QueryNetworkOutbound:
		return m.decideNetwork(true, q.Host, q.Port, q.Protocol)
	case QueryNetworkInbound:
		return m.decideNetwork(false, q.Host, q.Port, q.Protocol)
	case QueryExec:
		return m.decideExec(q.ResourceURI)
	case QueryMCPToolCall:
		return m.decideMCPTool(q.MCPServer, q.MCPTool)
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

func (m *Manifest) decideFsWrite(uri string, now, sessionStart time.Time) Decision {
	// Explicit-takes-precedence (ADR-019, issue #40): if any write_grant
	// names this resource for this action, that grant's time window is
	// decisive — broader tools.filesystem.write does NOT paper over an
	// expired explicit grant.
	switch state, g := m.classifyWriteGrant(uri, ActionWrite, now, sessionStart); state {
	case grantValid:
		return m.writeGrantDecision(uri, g, ActionWrite)
	case grantExpired:
		return denyDecision(fmt.Sprintf("filesystem write of %s blocked by expired write_grant", uri))
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

func (m *Manifest) decideFsDelete(uri string, now, sessionStart time.Time) Decision {
	switch state, g := m.classifyWriteGrant(uri, ActionDelete, now, sessionStart); state {
	case grantValid:
		return m.writeGrantDecision(uri, g, ActionDelete)
	case grantExpired:
		return denyDecision(fmt.Sprintf("filesystem delete of %s blocked by expired write_grant", uri))
	}
	return denyDecision(fmt.Sprintf("filesystem delete of %s not granted by any write_grant", uri))
}

// decideMCPTool mirrors aegis_policy::Policy::check_mcp_tool. Closed-by-
// default per ADR-018 / F2-MCP-B: allowed iff tools.mcp[] has an entry
// whose server_name matches AND whose allowed_tools contains tool_name.
func (m *Manifest) decideMCPTool(server, tool string) Decision {
	for i := range m.Tools.MCP {
		s := &m.Tools.MCP[i]
		if s.ServerName != server {
			continue
		}
		// ADR-024: AllowedTools entries are now AllowedTool (union of
		// string shorthand + object form with pre_validate clauses).
		// The MCP-allowlist check still only cares about the name —
		// the per-tool pre_validate clauses are consumed by the
		// mediator (ADR-024-B) after this allow/deny decision.
		for _, t := range s.AllowedTools {
			if t.Name == tool {
				return allowDecision()
			}
		}
		return denyDecision(fmt.Sprintf(
			"mcp tool call %q/%q not granted: tool not in allowed_tools", server, tool,
		))
	}
	return denyDecision(fmt.Sprintf(
		"mcp tool call to server %q not granted: server not in tools.mcp[]", server,
	))
}

func (m *Manifest) decideExec(program string) Decision {
	matched := false
	for _, g := range m.ExecGrants {
		if programMatches(g.Program, program) {
			matched = true
			break
		}
	}
	if !matched {
		return denyDecision(fmt.Sprintf("exec of %s not granted by manifest", program))
	}
	return upgradeForApproval(allowDecision(), m.ApprovalRequiredFor, ApprovalAnyExec,
		"any_exec requires approval")
}

// programMatches: slash-bearing grants are absolute paths matched
// exactly; bare basenames match the query's path.Base. MUST stay in
// lockstep with Rust's program_matches.
func programMatches(grant, query string) bool {
	if strings.Contains(grant, "/") {
		return grant == query
	}
	return filepath.Base(query) == grant
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

// grantState distinguishes the three relevant outcomes when looking
// up an explicit write_grant for a resource: no grant at all (fall
// through to broader rules), a time-valid grant (decisive), or one
// that exists but has expired (decisive Deny per ADR-019).
type grantState int

const (
	grantNone grantState = iota
	grantValid
	grantExpired
)

// classifyWriteGrant mirrors Rust's `Policy::classify_write_grant`. It
// scans `m.WriteGrants` for any entry naming `uri` for `action` and
// returns the strongest outcome — Valid wins over Expired wins over
// None. Drives the explicit-takes-precedence rule.
func (m *Manifest) classifyWriteGrant(
	uri string,
	action WriteAction,
	now, sessionStart time.Time,
) (grantState, *WriteGrant) {
	state := grantNone
	for i := range m.WriteGrants {
		g := &m.WriteGrants[i]
		if g.Resource != uri {
			continue
		}
		hasAction := false
		for _, a := range g.Actions {
			if a == string(action) {
				hasAction = true
				break
			}
		}
		if !hasAction {
			continue
		}
		if grantTimeValid(g, now, sessionStart) {
			return grantValid, g
		}
		state = grantExpired
	}
	return state, nil
}

// grantTimeValid mirrors aegis_policy::policy::grant_time_valid in Rust:
// expires_at is an absolute cut-off (RFC 3339); duration is relative to
// session_start (ISO-8601). Both must hold when both are present.
//
// If `now` or `sessionStart` are zero values, time bounds are skipped —
// callers without a clock anchor see pre-#38 behavior. Real callers
// (the F0 mediator) always pass real timestamps.
func grantTimeValid(g *WriteGrant, now, sessionStart time.Time) bool {
	if now.IsZero() {
		return true
	}
	if g.ExpiresAt != "" {
		exp, err := time.Parse(time.RFC3339, g.ExpiresAt)
		if err != nil {
			return false
		}
		if !now.Before(exp) {
			return false
		}
	}
	if g.Duration != "" {
		dur, ok := parseISO8601Duration(g.Duration)
		if !ok {
			return false
		}
		if sessionStart.IsZero() {
			return true
		}
		if now.Sub(sessionStart) >= dur {
			return false
		}
	}
	return true
}

// parseISO8601Duration accepts the form `P[<n>D][T[<n>H][<n>M][<n>S]]`
// with integer components. Mirrors the Rust parser. No fractional
// seconds, weeks, months, or years.
func parseISO8601Duration(s string) (time.Duration, bool) {
	if !strings.HasPrefix(s, "P") {
		return 0, false
	}
	s = s[1:]

	var datePart, timePart string
	if idx := strings.Index(s, "T"); idx >= 0 {
		datePart = s[:idx]
		timePart = s[idx+1:]
		if timePart == "" {
			// "P1DT" with empty time-part is malformed.
			return 0, false
		}
	} else {
		datePart = s
	}

	var total time.Duration
	if datePart != "" {
		if !strings.HasSuffix(datePart, "D") {
			return 0, false
		}
		n, err := strconv.Atoi(datePart[:len(datePart)-1])
		if err != nil || n < 0 {
			return 0, false
		}
		total += time.Duration(n) * 24 * time.Hour
	}

	for timePart != "" {
		idx := strings.IndexAny(timePart, "HMS")
		if idx < 0 {
			return 0, false
		}
		n, err := strconv.Atoi(timePart[:idx])
		if err != nil || n < 0 {
			return 0, false
		}
		switch timePart[idx] {
		case 'H':
			total += time.Duration(n) * time.Hour
		case 'M':
			total += time.Duration(n) * time.Minute
		case 'S':
			total += time.Duration(n) * time.Second
		}
		timePart = timePart[idx+1:]
	}

	return total, true
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
