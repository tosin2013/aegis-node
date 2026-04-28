package manifest

import (
	"encoding/json"
	"fmt"

	"gopkg.in/yaml.v3"
)

// UnmarshalYAML handles the `oneOf {string, object}` shape for network
// policy: the bare string "deny"/"allow", or an object with `allowlist`.
func (np *NetworkPolicy) UnmarshalYAML(value *yaml.Node) error {
	switch value.Kind {
	case yaml.ScalarNode:
		switch value.Value {
		case string(NetworkDeny):
			np.Mode = NetworkDeny
			return nil
		case string(NetworkAllow):
			np.Mode = NetworkAllow
			return nil
		default:
			return fmt.Errorf("network policy: unknown string value %q (want deny|allow)", value.Value)
		}
	case yaml.MappingNode:
		var raw struct {
			Allowlist []NetworkAllowEntry `yaml:"allowlist"`
		}
		if err := value.Decode(&raw); err != nil {
			return err
		}
		np.Mode = NetworkAllowlist
		np.Allowlist = raw.Allowlist
		return nil
	default:
		return fmt.Errorf("network policy: expected string or mapping at line %d col %d", value.Line, value.Column)
	}
}

// UnmarshalJSON handles the same `oneOf` for the JSON shape produced by
// json.Marshal(map[string]any) after schema validation.
func (np *NetworkPolicy) UnmarshalJSON(data []byte) error {
	var s string
	if err := json.Unmarshal(data, &s); err == nil {
		switch s {
		case string(NetworkDeny):
			np.Mode = NetworkDeny
			return nil
		case string(NetworkAllow):
			np.Mode = NetworkAllow
			return nil
		default:
			return fmt.Errorf("network policy: unknown string value %q", s)
		}
	}
	var raw struct {
		Allowlist []NetworkAllowEntry `json:"allowlist"`
	}
	if err := json.Unmarshal(data, &raw); err != nil {
		return fmt.Errorf("network policy: expected string or object: %w", err)
	}
	np.Mode = NetworkAllowlist
	np.Allowlist = raw.Allowlist
	return nil
}

// MarshalJSON emits the canonical JSON form so a parsed manifest can be
// re-serialized for cross-language conformance harnesses.
func (np NetworkPolicy) MarshalJSON() ([]byte, error) {
	if np.Mode == NetworkAllowlist {
		return json.Marshal(struct {
			Allowlist []NetworkAllowEntry `json:"allowlist"`
		}{Allowlist: np.Allowlist})
	}
	return json.Marshal(string(np.Mode))
}
