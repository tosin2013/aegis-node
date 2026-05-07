package manifest

import "testing"

func TestDottedPathToJSONPointer(t *testing.T) {
	cases := []struct {
		in, want string
	}{
		{"", ""},
		{"agent.name", "/agent/name"},
		{"tools.filesystem.write", "/tools/filesystem/write"},
		{"tools.filesystem.read[0]", "/tools/filesystem/read/0"},
		{"write_grants[2].resource", "/write_grants/2/resource"},
		{"matrix[1][2]", "/matrix/1/2"},
	}
	for _, c := range cases {
		if got := dottedPathToJSONPointer(c.in); got != c.want {
			t.Errorf("dottedPathToJSONPointer(%q) = %q, want %q", c.in, got, c.want)
		}
	}
}

func TestSplitBracketsRejectsNonNumeric(t *testing.T) {
	key, idx := splitBrackets("matrix[abc]")
	if key != "matrix" {
		t.Errorf("key = %q, want %q", key, "matrix")
	}
	if len(idx) != 0 {
		t.Errorf("non-numeric index should be skipped, got %v", idx)
	}
}

func TestLookupPositionRootFallback(t *testing.T) {
	// Empty field returns (1, 1) without parsing.
	if l, c := LookupPosition([]byte("a: 1\n"), ""); l != 1 || c != 1 {
		t.Errorf("empty field = (%d,%d), want (1,1)", l, c)
	}
}

func TestLookupPositionOnRealFields(t *testing.T) {
	yaml := `agent:
  name: smoke-test
  version: "1.0.0"
identity:
  spiffeId: spiffe://aegis-node.local/agent/smoke/inst-001
tools:
  filesystem:
    read:
      - /one/path
      - /two/path
    write:
      - /tmp/out
write_grants:
  - resource: /tmp/out/file.txt
    actions: [write]
`
	// Note: yaml.v3 returns the position of the VALUE node, not the
	// key node. For sequence-valued keys (e.g. `write:` whose value
	// is a multi-line list) that means the marker lands on the
	// first list item, one line below the key. UX-wise this is
	// fine — the squiggle highlights the offending content rather
	// than the field name. Tests assert the actual yaml.v3
	// behaviour; if we ever switch to key-position tracking, update
	// here in lockstep.
	cases := []struct {
		field          string
		wantLine       int
		wantNonZeroCol bool
	}{
		// Map-key descent — value on same line as key.
		{"agent.name", 2, true},
		{"identity.spiffeId", 5, true},
		// Sequence-valued key — value is the first element line.
		{"tools.filesystem.write", 12, true},
		// Array-index descent.
		{"tools.filesystem.read[0]", 9, true},
		{"tools.filesystem.read[1]", 10, true},
		{"write_grants[0].resource", 14, true},
	}
	for _, c := range cases {
		gotLine, gotCol := LookupPosition([]byte(yaml), c.field)
		if gotLine != c.wantLine {
			t.Errorf("LookupPosition(%q) line = %d, want %d", c.field, gotLine, c.wantLine)
		}
		if c.wantNonZeroCol && gotCol == 0 {
			t.Errorf("LookupPosition(%q) col = 0, want non-zero", c.field)
		}
	}
}

func TestLookupPositionUnknownPath(t *testing.T) {
	yaml := "agent:\n  name: x\n"
	if l, c := LookupPosition([]byte(yaml), "tools.does_not_exist"); l != 0 || c != 0 {
		t.Errorf("unknown path = (%d,%d), want (0,0)", l, c)
	}
}

func TestLookupPositionMalformedYAML(t *testing.T) {
	if l, c := LookupPosition([]byte("not: valid: yaml: ::\n"), "anything"); l != 0 || c != 0 {
		t.Errorf("malformed yaml = (%d,%d), want (0,0)", l, c)
	}
}
