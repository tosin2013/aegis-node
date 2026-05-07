package manifest

import (
	"strconv"
	"strings"

	"gopkg.in/yaml.v3"
)

// LookupPosition returns the 1-indexed line + column of the YAML
// node at the given dotted-with-brackets field path inside `yamlBytes`.
// Returns `(0, 0)` if the path can't be resolved (caller treats this
// as "no precise position, fall back to line 1").
//
// Path syntax matches what `pkg/validate.Finding.Field` produces:
//
//   - dotted segments traverse maps   ("agent.name", "tools.filesystem.write")
//   - "[N]" suffixes traverse arrays  ("write_grants[0].resource")
//   - "" or "/" returns the root position
//
// Used by `cmd/aegis/validate.go` after `validate.Lint` to attach
// real source positions to each Finding so output formats (JSON,
// GitHub Annotations, text) can produce `file:line:col` rather than
// the placeholder `file:0:0` that pre-position-aware versions emit.
func LookupPosition(yamlBytes []byte, fieldPath string) (line, col int) {
	if fieldPath == "" {
		return 1, 1
	}
	var root yaml.Node
	if err := yaml.Unmarshal(yamlBytes, &root); err != nil {
		return 0, 0
	}
	docNode := &root
	if root.Kind == yaml.DocumentNode && len(root.Content) > 0 {
		docNode = root.Content[0]
	}
	pointer := dottedPathToJSONPointer(fieldPath)
	l, c := lineColAtJSONPointer(docNode, pointer)
	// The yaml.v3 library uses 1-indexed line/col; translate the
	// "couldn't resolve" sentinel (returns the root's position) into
	// (0, 0) when the requested pointer didn't actually match a node.
	// We can't distinguish "matched root" from "fell back to root"
	// by return value alone; treat root-position as a failure when
	// the original path was non-trivial. Acceptable trade-off — root
	// is rarely the actual finding location.
	if pointer != "" && pointer != "/" && (l == docNode.Line && c == docNode.Column) {
		// Path didn't resolve to a deeper node. Surface as "unknown."
		// Callers fall back to line 1.
		return 0, 0
	}
	return l, c
}

// dottedPathToJSONPointer converts a path like "write_grants[0].resource"
// into RFC 6901 form "/write_grants/0/resource" suitable for
// lineColAtJSONPointer. Empty input → "".
func dottedPathToJSONPointer(field string) string {
	if field == "" {
		return ""
	}
	var b strings.Builder
	dotSegments := strings.Split(field, ".")
	for _, seg := range dotSegments {
		// Split off any "[N]" suffixes; a segment can carry zero,
		// one, or several brackets (rare but supported).
		key, indices := splitBrackets(seg)
		if key != "" {
			b.WriteByte('/')
			b.WriteString(escapePointer(key))
		}
		for _, idx := range indices {
			b.WriteByte('/')
			b.WriteString(idx)
		}
	}
	return b.String()
}

// splitBrackets parses a segment like "read[2]" or "write_grants[0]"
// into ("read", ["2"]) / ("write_grants", ["0"]). Multi-bracket
// segments like "matrix[1][2]" produce ("matrix", ["1", "2"]).
func splitBrackets(seg string) (key string, indices []string) {
	open := strings.Index(seg, "[")
	if open == -1 {
		return seg, nil
	}
	key = seg[:open]
	rest := seg[open:]
	for len(rest) > 0 {
		if rest[0] != '[' {
			break
		}
		close := strings.Index(rest, "]")
		if close == -1 {
			break
		}
		// Validate that the bracket content is a non-negative integer
		// — silently skip non-numeric to avoid producing broken
		// pointer segments. Validate-rule code uses this format
		// consistently so non-numeric brackets never appear.
		inner := rest[1:close]
		if _, err := strconv.Atoi(inner); err == nil {
			indices = append(indices, inner)
		}
		rest = rest[close+1:]
	}
	return key, indices
}

// escapePointer applies RFC 6901 escapes so map keys containing
// `/` or `~` survive the round-trip. Manifest field names don't
// usually contain either, but keep the function honest for
// future-proofing.
func escapePointer(s string) string {
	if !strings.ContainsAny(s, "/~") {
		return s
	}
	r := strings.NewReplacer("~", "~0", "/", "~1")
	return r.Replace(s)
}
