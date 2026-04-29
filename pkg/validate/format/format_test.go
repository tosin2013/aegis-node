package format

import (
	"bytes"
	"encoding/json"
	"encoding/xml"
	"strings"
	"testing"

	"github.com/tosin2013/aegis-node/pkg/validate"
)

// fixtureFindings returns three findings spanning all three severities,
// in deterministic order. Used by every formatter test so output stays
// comparable across formats.
func fixtureFindings() []validate.Finding {
	return []validate.Finding{
		{
			RuleID:    "AEGIS001",
			Severity:  validate.SeverityError,
			Field:     "tools.filesystem.read[0]",
			Message:   "path \"/etc\" is a system root",
			Rationale: "Blanket read of system roots gives the agent access to credentials.",
		},
		{
			RuleID:    "AEGIS006",
			Severity:  validate.SeverityWarn,
			Field:     "write_grants[0]",
			Message:   "grant on \"/data/scratch.md\" has no duration and no expires_at",
			Rationale: "Without a time bound, write_grants live forever.",
		},
		{
			RuleID:    "AEGIS010",
			Severity:  validate.SeverityInfo,
			Field:     "agent.name",
			Message:   "agent.name=\"foo\" doesn't match SPIFFE workload segment \"bar\"",
			Rationale: "Convention is spiffe://<td>/agent/<workload>/<instance>.",
		},
	}
}

func TestParseRejectsUnknown(t *testing.T) {
	if _, err := Parse("yaml"); err == nil {
		t.Fatal("Parse should reject unknown formats")
	}
	for _, valid := range []string{"text", "github", "junit", "json"} {
		if _, err := Parse(valid); err != nil {
			t.Errorf("Parse(%q) failed: %v", valid, err)
		}
	}
}

func TestRenderTextSummaryAndCounts(t *testing.T) {
	var buf bytes.Buffer
	if err := Render(&buf, "manifest.yaml", fixtureFindings(), FormatText); err != nil {
		t.Fatalf("Render: %v", err)
	}
	got := buf.String()
	for _, want := range []string{
		"AEGIS001",
		"AEGIS006",
		"AEGIS010",
		"manifest.yaml:0:0:",
		"1 error(s), 1 warning(s), 1 info",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("text output missing %q\nfull output:\n%s", want, got)
		}
	}
}

func TestRenderTextEmptyIsClean(t *testing.T) {
	var buf bytes.Buffer
	if err := Render(&buf, "ok.yaml", nil, FormatText); err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(buf.String(), "clean — 0 findings") {
		t.Errorf("text empty output: %q", buf.String())
	}
}

func TestRenderGitHubAnnotations(t *testing.T) {
	var buf bytes.Buffer
	if err := Render(&buf, "manifest.yaml", fixtureFindings(), FormatGitHub); err != nil {
		t.Fatalf("Render: %v", err)
	}
	got := buf.String()
	// Each level uses the right GHA prefix.
	if !strings.Contains(got, "::error file=manifest.yaml,") {
		t.Errorf("missing ::error annotation:\n%s", got)
	}
	if !strings.Contains(got, "::warning file=manifest.yaml,") {
		t.Errorf("missing ::warning annotation:\n%s", got)
	}
	if !strings.Contains(got, "::notice file=manifest.yaml,") {
		t.Errorf("missing ::notice annotation:\n%s", got)
	}
	// Special characters in messages are encoded.
	if strings.Contains(got, "\n") && strings.Count(got, "\n") < 3 {
		// Should have one newline per annotation, no embedded ones.
		t.Errorf("GHA annotations should be one-per-line:\n%s", got)
	}
}

func TestRenderGitHubEncodesSpecials(t *testing.T) {
	findings := []validate.Finding{{
		RuleID:   "AEGIS001",
		Severity: validate.SeverityError,
		Field:    "x",
		Message:  "value, has commas: and colons",
	}}
	var buf bytes.Buffer
	if err := Render(&buf, "m.yaml", findings, FormatGitHub); err != nil {
		t.Fatalf("Render: %v", err)
	}
	got := buf.String()
	if strings.Contains(got, "value, has commas:") {
		t.Errorf("commas/colons in message must be percent-encoded:\n%s", got)
	}
	if !strings.Contains(got, "%2C") || !strings.Contains(got, "%3A") {
		t.Errorf("expected %%2C and %%3A in encoded message:\n%s", got)
	}
}

func TestRenderJUnitParsesAsXML(t *testing.T) {
	var buf bytes.Buffer
	if err := Render(&buf, "manifest.yaml", fixtureFindings(), FormatJUnit); err != nil {
		t.Fatalf("Render: %v", err)
	}
	var suite junitTestSuite
	if err := xml.Unmarshal(stripXMLHeader(buf.Bytes()), &suite); err != nil {
		t.Fatalf("XML reparse failed: %v\nraw:\n%s", err, buf.String())
	}
	if suite.Name != "aegis-validate" {
		t.Errorf("suite name: %q", suite.Name)
	}
	if suite.Tests != 3 {
		t.Errorf("tests: got %d want 3", suite.Tests)
	}
	if suite.Errors != 1 {
		t.Errorf("errors: got %d want 1", suite.Errors)
	}
	if suite.Failures != 1 {
		t.Errorf("failures: got %d want 1", suite.Failures)
	}
}

func TestRenderJUnitCleanCaseEmitsOnePass(t *testing.T) {
	var buf bytes.Buffer
	if err := Render(&buf, "ok.yaml", nil, FormatJUnit); err != nil {
		t.Fatalf("Render: %v", err)
	}
	var suite junitTestSuite
	if err := xml.Unmarshal(stripXMLHeader(buf.Bytes()), &suite); err != nil {
		t.Fatalf("XML reparse failed: %v", err)
	}
	if suite.Tests != 1 || suite.Failures != 0 || suite.Errors != 0 {
		t.Errorf("clean junit: got tests=%d failures=%d errors=%d (want 1/0/0)",
			suite.Tests, suite.Failures, suite.Errors)
	}
}

func TestRenderJSONRoundTripsAgainstSchema(t *testing.T) {
	var buf bytes.Buffer
	if err := Render(&buf, "manifest.yaml", fixtureFindings(), FormatJSON); err != nil {
		t.Fatalf("Render: %v", err)
	}
	// Each line is one record.
	lines := strings.Split(strings.TrimRight(buf.String(), "\n"), "\n")
	if len(lines) != 3 {
		t.Fatalf("expected 3 records, got %d:\n%s", len(lines), buf.String())
	}
	for i, line := range lines {
		var rec jsonRecord
		if err := json.Unmarshal([]byte(line), &rec); err != nil {
			t.Errorf("record %d not valid JSON: %v", i, err)
		}
		if rec.RuleID == "" || rec.Severity == "" || rec.Field == "" || rec.Message == "" {
			t.Errorf("record %d missing required field: %+v", i, rec)
		}
		if rec.File != "manifest.yaml" {
			t.Errorf("record %d file: %q", i, rec.File)
		}
	}
}

func TestRenderStableSortAcrossFormats(t *testing.T) {
	// Reverse-shuffle the input; output order must still be by (Field, RuleID).
	in := fixtureFindings()
	for i, j := 0, len(in)-1; i < j; i, j = i+1, j-1 {
		in[i], in[j] = in[j], in[i]
	}
	for _, fmtKind := range []Format{FormatText, FormatGitHub, FormatJUnit, FormatJSON} {
		var a, b bytes.Buffer
		if err := Render(&a, "m.yaml", in, fmtKind); err != nil {
			t.Fatalf("%s: %v", fmtKind, err)
		}
		// Same content, original order — should produce identical output.
		original := fixtureFindings()
		if err := Render(&b, "m.yaml", original, fmtKind); err != nil {
			t.Fatalf("%s: %v", fmtKind, err)
		}
		if a.String() != b.String() {
			t.Errorf("%s: output differs after re-ordering input — sort is not stable\nshuffled:\n%s\noriginal:\n%s",
				fmtKind, a.String(), b.String())
		}
	}
}

// stripXMLHeader drops the `<?xml ...?>` line so xml.Unmarshal can
// handle the body alone.
func stripXMLHeader(b []byte) []byte {
	if i := bytes.Index(b, []byte("?>")); i >= 0 {
		return b[i+2:]
	}
	return b
}
