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

import (
	"encoding/json"
	"fmt"

	"gopkg.in/yaml.v3"
)

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
	// ApprovalAuthorities lists SPIFFE IDs allowed to issue approvals over
	// the F3 mTLS signed-API channel (ADR-005, ADR-003, issue #36). Empty
	// or absent => mTLS approvals are refused outright.
	ApprovalAuthorities []string `yaml:"approval_authorities,omitempty" json:"approval_authorities,omitempty"`
	// Inference is the ADR-014 / LLM-C configuration block. Additive;
	// nil means "backend defaults."
	Inference *Inference `yaml:"inference,omitempty" json:"inference,omitempty"`
}

// Inference is the inference-time configuration block (per ADR-014,
// LLM-C / issue #72). Currently carries determinism knobs only.
type Inference struct {
	Determinism *DeterminismKnobs `yaml:"determinism,omitempty" json:"determinism,omitempty"`
}

// DeterminismKnobs are the sampling-time knobs LLM-C surfaces through
// the manifest. All fields are pointers so absence ("backend default
// for that knob") and explicit zero values stay distinguishable.
// Setting Seed and Temperature=0.0 together yields byte-identical
// output across runs — the configuration auditors rely on for replay
// verification.
type DeterminismKnobs struct {
	Seed          *uint32  `yaml:"seed,omitempty" json:"seed,omitempty"`
	Temperature   *float32 `yaml:"temperature,omitempty" json:"temperature,omitempty"`
	TopP          *float32 `yaml:"top_p,omitempty" json:"top_p,omitempty"`
	TopK          *uint32  `yaml:"top_k,omitempty" json:"top_k,omitempty"`
	RepeatPenalty *float32 `yaml:"repeat_penalty,omitempty" json:"repeat_penalty,omitempty"`
}

type Agent struct {
	Name    string `yaml:"name" json:"name"`
	Version string `yaml:"version" json:"version"`
}

type Identity struct {
	SpiffeID string `yaml:"spiffeId" json:"spiffeId"`
}

type Tools struct {
	Filesystem *Filesystem      `yaml:"filesystem,omitempty" json:"filesystem,omitempty"`
	Network    *Network         `yaml:"network,omitempty" json:"network,omitempty"`
	APIs       []APIGrant       `yaml:"apis,omitempty" json:"apis,omitempty"`
	MCP        []MCPServerGrant `yaml:"mcp,omitempty" json:"mcp,omitempty"`
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

// MCPServerGrant is one entry in `tools.mcp` (per ADR-018). The agent may
// connect to `ServerURI` and invoke any tool name listed in `AllowedTools`.
// Closed-by-default: an MCP tool call against a server not listed here is
// denied + emits a Violation per F2.
type MCPServerGrant struct {
	ServerName   string        `yaml:"server_name" json:"server_name"`
	ServerURI    string        `yaml:"server_uri" json:"server_uri"`
	AllowedTools []AllowedTool `yaml:"allowed_tools" json:"allowed_tools"`
}

// AllowedTool is one entry in MCPServerGrant.AllowedTools (per ADR-024-A).
// Two shapes are accepted at parse time:
//
//  1. String shorthand — `"read_text_file"` — interpreted as
//     "no pre-validation; one-layer enforcement," preserving the
//     pre-ADR-024 behavior. Stored with Name set, PreValidate empty.
//  2. Object form — `{ name, pre_validate }` — declares side-effect
//     clauses the mediator runs against `tools.filesystem.*` /
//     `tools.network.*` policy before dispatching to the MCP server.
//
// The Rust side uses `#[serde(untagged)]`; here we implement custom
// UnmarshalYAML / UnmarshalJSON that try the string form first, then
// fall back to the object form. MarshalJSON / MarshalYAML serialize
// back into whichever shape preserves the original input (string when
// PreValidate is empty, object otherwise) so round-trips are stable.
type AllowedTool struct {
	// Tool name (matches the MCP server's tool catalog).
	Name string `json:"name" yaml:"name"`
	// Side-effect clauses the mediator pre-validates before dispatch.
	// Empty / absent collapses to the string-shorthand semantics.
	PreValidate []PreValidateClause `json:"pre_validate,omitempty" yaml:"pre_validate,omitempty"`
}

// UnmarshalJSON accepts either a JSON string or a JSON object.
func (a *AllowedTool) UnmarshalJSON(data []byte) error {
	// Strip leading whitespace per JSON spec; the first non-space byte
	// tells us which branch to take.
	for _, b := range data {
		switch b {
		case ' ', '\t', '\n', '\r':
			continue
		case '"':
			// String shorthand.
			var s string
			if err := json.Unmarshal(data, &s); err != nil {
				return fmt.Errorf("allowed_tools entry: %w", err)
			}
			a.Name = s
			a.PreValidate = nil
			return nil
		case '{':
			// Object form. Use a typed shadow to avoid recursing into
			// AllowedTool.UnmarshalJSON.
			type allowedToolObj struct {
				Name        string              `json:"name"`
				PreValidate []PreValidateClause `json:"pre_validate"`
			}
			var obj allowedToolObj
			if err := json.Unmarshal(data, &obj); err != nil {
				return fmt.Errorf("allowed_tools entry: %w", err)
			}
			if obj.Name == "" {
				return fmt.Errorf("allowed_tools entry: object form requires non-empty `name`")
			}
			a.Name = obj.Name
			a.PreValidate = obj.PreValidate
			return nil
		default:
			return fmt.Errorf("allowed_tools entry: expected string or object, got %q", string(b))
		}
	}
	return fmt.Errorf("allowed_tools entry: empty value")
}

// MarshalJSON emits the shorthand string form when PreValidate is
// empty; otherwise emits the object form. Round-trips are stable as
// long as the input used the same shape.
func (a AllowedTool) MarshalJSON() ([]byte, error) {
	if len(a.PreValidate) == 0 {
		return json.Marshal(a.Name)
	}
	type allowedToolObj struct {
		Name        string              `json:"name"`
		PreValidate []PreValidateClause `json:"pre_validate"`
	}
	return json.Marshal(allowedToolObj{Name: a.Name, PreValidate: a.PreValidate})
}

// UnmarshalYAML accepts either a YAML scalar (string) or a YAML
// mapping (object). Mirrors UnmarshalJSON's behavior — the parsed
// shape is the same regardless of which surface delivered it.
func (a *AllowedTool) UnmarshalYAML(value *yaml.Node) error {
	switch value.Kind {
	case yaml.ScalarNode:
		// String shorthand. Reject non-string scalars (numbers, bools,
		// nulls) explicitly — those would be a manifest authoring error.
		if value.Tag != "" && value.Tag != "!!str" {
			return fmt.Errorf("allowed_tools entry: scalar must be a string, got tag %q", value.Tag)
		}
		a.Name = value.Value
		a.PreValidate = nil
		return nil
	case yaml.MappingNode:
		// Object form. Decode into a shadow type to avoid recursion.
		type allowedToolObj struct {
			Name        string              `yaml:"name"`
			PreValidate []PreValidateClause `yaml:"pre_validate"`
		}
		var obj allowedToolObj
		if err := value.Decode(&obj); err != nil {
			return fmt.Errorf("allowed_tools entry: %w", err)
		}
		if obj.Name == "" {
			return fmt.Errorf("allowed_tools entry: object form requires non-empty `name`")
		}
		a.Name = obj.Name
		a.PreValidate = obj.PreValidate
		return nil
	default:
		return fmt.Errorf("allowed_tools entry: expected scalar or mapping, got node kind %d", value.Kind)
	}
}

// MarshalYAML emits the shorthand string form when PreValidate is
// empty; otherwise emits the object form (parallel to MarshalJSON).
func (a AllowedTool) MarshalYAML() (interface{}, error) {
	if len(a.PreValidate) == 0 {
		return a.Name, nil
	}
	type allowedToolObj struct {
		Name        string              `yaml:"name"`
		PreValidate []PreValidateClause `yaml:"pre_validate"`
	}
	return allowedToolObj{Name: a.Name, PreValidate: a.PreValidate}, nil
}

// PreValidateClause is one side-effect-shaped pre-validation clause
// for an AllowedTool object form (per ADR-024 §"Decision" item 2).
// Phase 1 covers filesystem_{read,write,delete} + network_outbound.
//
// Exactly one of Arg / ArgArray must be set; the JSON Schema enforces
// this via `oneOf`. The Go validator surfaces a typed error if both
// or neither are present (see validate/rules.go).
type PreValidateClause struct {
	// Side-effect family this clause gates against.
	Kind PreValidateKind `yaml:"kind" json:"kind"`
	// Name of the scalar argument carrying the path or URL the
	// mediator should extract and check.
	Arg string `yaml:"arg,omitempty" json:"arg,omitempty"`
	// Name of an array-of-strings argument; the mediator extracts
	// each element and runs the check on it.
	ArgArray string `yaml:"arg_array,omitempty" json:"arg_array,omitempty"`
}

// PreValidateKind enumerates the side-effect families a
// PreValidateClause can gate against. Adding a new kind requires
// (a) a constant here, (b) the JSON Schema enum bump, (c) the
// matching policy.check_* method on the Rust side.
type PreValidateKind string

const (
	PreValidateFilesystemRead   PreValidateKind = "filesystem_read"
	PreValidateFilesystemWrite  PreValidateKind = "filesystem_write"
	PreValidateFilesystemDelete PreValidateKind = "filesystem_delete"
	PreValidateNetworkOutbound  PreValidateKind = "network_outbound"
)

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
