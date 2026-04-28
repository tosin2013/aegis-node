package manifest

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/santhosh-tekuri/jsonschema/v5"
	"gopkg.in/yaml.v3"
)

// Load reads a manifest from disk and returns the parsed Manifest after
// schema validation. Returns a *ParseError with file/line/column on the
// first failure encountered.
//
// `extends:` references are NOT followed here — call [LoadResolved] for the
// fully-resolved form including parent narrowing checks.
func Load(path string) (*Manifest, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("manifest: read %s: %w", path, err)
	}
	return Parse(path, data)
}

// Parse validates `data` against the embedded schema and returns the parsed
// Manifest. `path` is used only for error messages (use "" for in-memory).
func Parse(path string, data []byte) (*Manifest, error) {
	var root yaml.Node
	if err := yaml.Unmarshal(data, &root); err != nil {
		return nil, &ParseError{
			Path:    path,
			Message: fmt.Sprintf("invalid YAML: %v", err),
		}
	}

	// `yaml.Unmarshal` returns a DocumentNode wrapper; unwrap to the actual
	// content (typically a MappingNode).
	docNode := &root
	if root.Kind == yaml.DocumentNode && len(root.Content) > 0 {
		docNode = root.Content[0]
	}

	val, err := nodeToAny(docNode)
	if err != nil {
		return nil, &ParseError{
			Path:    path,
			Line:    docNode.Line,
			Column:  docNode.Column,
			Message: err.Error(),
		}
	}

	sch, err := loadSchema()
	if err != nil {
		return nil, err
	}
	if vErr := sch.Validate(val); vErr != nil {
		return nil, schemaErrorToParseError(path, docNode, vErr)
	}

	jsonBytes, err := json.Marshal(val)
	if err != nil {
		return nil, fmt.Errorf("manifest: re-encode validated tree: %w", err)
	}
	var m Manifest
	dec := json.NewDecoder(strings.NewReader(string(jsonBytes)))
	dec.DisallowUnknownFields()
	if err := dec.Decode(&m); err != nil {
		// Schema already passed, so this means a struct/schema drift bug.
		return nil, fmt.Errorf("manifest: post-validation decode: %w", err)
	}
	return &m, nil
}

// nodeToAny converts a YAML node tree into the generic `any` shape JSON
// Schema validators expect. Tags (`!!str`, `!!int`, …) determine the Go
// scalar type so e.g. `port: 443` becomes a float64 (jsonschema/v5 treats
// numbers as float64 to match encoding/json's default).
func nodeToAny(n *yaml.Node) (any, error) {
	switch n.Kind {
	case yaml.ScalarNode:
		return scalarToAny(n)
	case yaml.SequenceNode:
		out := make([]any, 0, len(n.Content))
		for _, c := range n.Content {
			v, err := nodeToAny(c)
			if err != nil {
				return nil, err
			}
			out = append(out, v)
		}
		return out, nil
	case yaml.MappingNode:
		out := make(map[string]any, len(n.Content)/2)
		for i := 0; i+1 < len(n.Content); i += 2 {
			k := n.Content[i]
			v := n.Content[i+1]
			if k.Kind != yaml.ScalarNode {
				return nil, fmt.Errorf("manifest: non-scalar map key at line %d col %d", k.Line, k.Column)
			}
			val, err := nodeToAny(v)
			if err != nil {
				return nil, err
			}
			out[k.Value] = val
		}
		return out, nil
	case yaml.AliasNode:
		if n.Alias == nil {
			return nil, fmt.Errorf("manifest: dangling alias at line %d", n.Line)
		}
		return nodeToAny(n.Alias)
	default:
		return nil, fmt.Errorf("manifest: unsupported YAML node kind at line %d", n.Line)
	}
}

func scalarToAny(n *yaml.Node) (any, error) {
	switch n.Tag {
	case "!!null", "":
		// Empty tag: yaml.v3 sometimes leaves Tag empty; fall through to
		// !!str default if Value is non-empty.
		if n.Tag == "!!null" || n.Value == "null" || n.Value == "~" || n.Value == "" {
			return nil, nil
		}
		return n.Value, nil
	case "!!str":
		return n.Value, nil
	case "!!int":
		i, err := strconv.ParseInt(n.Value, 10, 64)
		if err != nil {
			return nil, fmt.Errorf("manifest: bad integer %q at line %d", n.Value, n.Line)
		}
		return float64(i), nil // JSON-Schema-friendly numeric type.
	case "!!float":
		f, err := strconv.ParseFloat(n.Value, 64)
		if err != nil {
			return nil, fmt.Errorf("manifest: bad float %q at line %d", n.Value, n.Line)
		}
		return f, nil
	case "!!bool":
		switch strings.ToLower(n.Value) {
		case "true", "yes", "on":
			return true, nil
		case "false", "no", "off":
			return false, nil
		}
		return nil, fmt.Errorf("manifest: bad bool %q at line %d", n.Value, n.Line)
	default:
		return n.Value, nil
	}
}

// schemaErrorToParseError takes a jsonschema.ValidationError and walks the
// yaml.Node tree to attach a line/col to the deepest cause it can locate.
func schemaErrorToParseError(path string, root *yaml.Node, vErr error) *ParseError {
	ve, ok := vErr.(*jsonschema.ValidationError)
	if !ok {
		return &ParseError{Path: path, Message: vErr.Error()}
	}
	leaf := deepestCause(ve)
	line, col := lineColAtJSONPointer(root, leaf.InstanceLocation)
	msg := leaf.Message
	if msg == "" {
		msg = leaf.Error()
	}
	return &ParseError{
		Path:    path,
		Line:    line,
		Column:  col,
		Field:   leaf.InstanceLocation,
		Message: msg,
	}
}

func deepestCause(ve *jsonschema.ValidationError) *jsonschema.ValidationError {
	leaf := ve
	for len(leaf.Causes) > 0 {
		// Pick the cause with the longest InstanceLocation — most specific.
		best := leaf.Causes[0]
		for _, c := range leaf.Causes[1:] {
			if len(c.InstanceLocation) > len(best.InstanceLocation) {
				best = c
			}
		}
		leaf = best
	}
	return leaf
}

// lineColAtJSONPointer returns the 1-indexed line/column of the YAML node
// pointed at by `pointer` (RFC 6901 form: "/tools/filesystem/read/0").
// Returns the root node's position if the pointer can't be fully resolved.
func lineColAtJSONPointer(root *yaml.Node, pointer string) (int, int) {
	cur := root
	if pointer == "" || pointer == "/" {
		return cur.Line, cur.Column
	}
	parts := strings.Split(strings.TrimPrefix(pointer, "/"), "/")
	for _, raw := range parts {
		seg := decodePointerSegment(raw)
		next := childByKeyOrIndex(cur, seg)
		if next == nil {
			return cur.Line, cur.Column
		}
		cur = next
	}
	return cur.Line, cur.Column
}

func decodePointerSegment(s string) string {
	s = strings.ReplaceAll(s, "~1", "/")
	s = strings.ReplaceAll(s, "~0", "~")
	return s
}

func childByKeyOrIndex(n *yaml.Node, seg string) *yaml.Node {
	switch n.Kind {
	case yaml.MappingNode:
		for i := 0; i+1 < len(n.Content); i += 2 {
			if n.Content[i].Kind == yaml.ScalarNode && n.Content[i].Value == seg {
				return n.Content[i+1]
			}
		}
	case yaml.SequenceNode:
		idx, err := strconv.Atoi(seg)
		if err != nil || idx < 0 || idx >= len(n.Content) {
			return nil
		}
		return n.Content[idx]
	}
	return nil
}

// ResolveExtendsPath resolves a child manifest's `extends:` entry relative
// to the child's directory (so manifests can reference siblings via plain
// filenames). Absolute paths are honored as-is.
func ResolveExtendsPath(childPath, ref string) string {
	if filepath.IsAbs(ref) {
		return ref
	}
	return filepath.Join(filepath.Dir(childPath), ref)
}
