package manifest

import "fmt"

// ParseError carries a file-path / line / column tuple for editor-friendly
// diagnostics. Constructed by [Load] / [Parse] when YAML decoding or JSON
// Schema validation fails.
type ParseError struct {
	// Path is the on-disk file the error refers to (empty for in-memory bytes).
	Path string
	// Line and Column are 1-indexed (yaml.Node convention).
	Line   int
	Column int
	// Field is the JSON Pointer location of the offending value, e.g.
	// "/tools/filesystem/read/0".
	Field string
	// Message is the human-readable description.
	Message string
	// Suggestion is an optional fix hint (e.g. "did you mean 'tools'?").
	Suggestion string
}

func (e *ParseError) Error() string {
	loc := e.Path
	if loc == "" {
		loc = "<input>"
	}
	if e.Line > 0 {
		loc = fmt.Sprintf("%s:%d:%d", loc, e.Line, e.Column)
	}
	out := fmt.Sprintf("%s: %s", loc, e.Message)
	if e.Field != "" {
		out += fmt.Sprintf(" (at %s)", e.Field)
	}
	if e.Suggestion != "" {
		out += "\n  hint: " + e.Suggestion
	}
	return out
}

// NarrowingError is reported when a child manifest tries to widen a
// parent's permissions (per ADR-012). Always points at the child manifest;
// the parent path is named in the message.
type NarrowingError struct {
	ChildPath  string
	ParentPath string
	// Field is a dotted permission path: e.g. "tools.filesystem.read",
	// "approval_required_for", "write_grants[/data/x].actions".
	Field   string
	Message string
}

func (e *NarrowingError) Error() string {
	return fmt.Sprintf(
		"%s: child manifest exceeds parent %s at %s: %s",
		e.ChildPath, e.ParentPath, e.Field, e.Message,
	)
}

// CycleError is reported when extends: forms a cycle.
type CycleError struct {
	Chain []string
}

func (e *CycleError) Error() string {
	return fmt.Sprintf("extends: cycle detected: %v", e.Chain)
}
