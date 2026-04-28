// Package manifest implements the Aegis-Node Permission Manifest (F2).
//
// Per ADR-004 + ADR-009 + ADR-012. Manifests are the single source of truth
// for what an agent may do — closed-by-default, versioned, composable via
// `extends:`. This package owns parsing, schema validation, and verifying
// that a child manifest never widens its parent's permissions.
//
// The schema lives at schemas/manifest/v1/manifest.schema.json and is
// embedded at build time so the validator is self-contained.
package manifest

// Manifest mirrors the schema at schemas/manifest/v1/manifest.schema.json.
// Field names use the schema's spelling (camelCase for keys named in the
// JSON-LD ledger context, snake_case for fields the schema spells that way).
type Manifest struct {
	SchemaVersion       string          `yaml:"schemaVersion" json:"schemaVersion"`
	Agent               Agent           `yaml:"agent" json:"agent"`
	Identity            Identity        `yaml:"identity" json:"identity"`
	Extends             []string        `yaml:"extends,omitempty" json:"extends,omitempty"`
	Tools               Tools           `yaml:"tools" json:"tools"`
	WriteGrants         []WriteGrant    `yaml:"write_grants,omitempty" json:"write_grants,omitempty"`
	ApprovalRequiredFor []ApprovalClass `yaml:"approval_required_for,omitempty" json:"approval_required_for,omitempty"`
	ExecGrants          []ExecGrant     `yaml:"exec_grants,omitempty" json:"exec_grants,omitempty"`
}

type Agent struct {
	Name    string `yaml:"name" json:"name"`
	Version string `yaml:"version" json:"version"`
}

type Identity struct {
	SpiffeID string `yaml:"spiffeId" json:"spiffeId"`
}

type Tools struct {
	Filesystem *Filesystem `yaml:"filesystem,omitempty" json:"filesystem,omitempty"`
	Network    *Network    `yaml:"network,omitempty" json:"network,omitempty"`
	APIs       []APIGrant  `yaml:"apis,omitempty" json:"apis,omitempty"`
}

type Filesystem struct {
	Read  []string `yaml:"read,omitempty" json:"read,omitempty"`
	Write []string `yaml:"write,omitempty" json:"write,omitempty"`
}

type Network struct {
	Outbound *NetworkPolicy `yaml:"outbound,omitempty" json:"outbound,omitempty"`
	Inbound  *NetworkPolicy `yaml:"inbound,omitempty" json:"inbound,omitempty"`
}

// NetworkMode captures the schema's `oneOf {string enum, allowlist object}`.
type NetworkMode string

const (
	NetworkDeny      NetworkMode = "deny"
	NetworkAllow     NetworkMode = "allow"
	NetworkAllowlist NetworkMode = "allowlist"
)

// NetworkPolicy is the parsed form of the schema's `networkPolicy` definition.
// `Mode == NetworkAllowlist` ⇒ `Allowlist` is the source of truth and
// `Mode` is the marker; the other modes have empty `Allowlist`.
type NetworkPolicy struct {
	Mode      NetworkMode         `yaml:"-" json:"-"`
	Allowlist []NetworkAllowEntry `yaml:"allowlist,omitempty" json:"allowlist,omitempty"`
}

type NetworkAllowEntry struct {
	Host     string `yaml:"host" json:"host"`
	Port     int    `yaml:"port,omitempty" json:"port,omitempty"`
	Protocol string `yaml:"protocol,omitempty" json:"protocol,omitempty"`
}

type APIGrant struct {
	Name    string   `yaml:"name" json:"name"`
	Methods []string `yaml:"methods,omitempty" json:"methods,omitempty"`
}

type WriteGrant struct {
	Resource         string   `yaml:"resource" json:"resource"`
	Actions          []string `yaml:"actions" json:"actions"`
	Duration         string   `yaml:"duration,omitempty" json:"duration,omitempty"`
	ExpiresAt        string   `yaml:"expires_at,omitempty" json:"expires_at,omitempty"`
	ApprovalRequired bool     `yaml:"approval_required,omitempty" json:"approval_required,omitempty"`
}

// ExecGrant is one entry in `exec_grants`. `Program` may be an absolute
// path (matched exactly) or a bare basename (matches any path with that
// file name). `ArgsMatch` is parsed in Phase 1 and enforced once the
// runtime can pass argv to the gate.
type ExecGrant struct {
	Program   string `yaml:"program" json:"program"`
	ArgsMatch string `yaml:"args_match,omitempty" json:"args_match,omitempty"`
}

// ApprovalClass enumerates the valid string values of `approval_required_for`.
type ApprovalClass string

const (
	ApprovalAnyWrite           ApprovalClass = "any_write"
	ApprovalAnyDelete          ApprovalClass = "any_delete"
	ApprovalAnyNetworkOutbound ApprovalClass = "any_network_outbound"
	ApprovalAnyExec            ApprovalClass = "any_exec"
)
