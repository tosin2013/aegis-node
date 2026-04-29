package validate

import (
	"sort"
	"strings"
	"testing"

	"github.com/tosin2013/aegis-node/pkg/manifest"
)

// loadFixture parses a testdata YAML and returns the manifest. Each
// fixture is small + self-contained; failures here are test-setup bugs.
func loadFixture(t *testing.T, name string) *manifest.Manifest {
	t.Helper()
	m, err := manifest.Load("testdata/" + name)
	if err != nil {
		t.Fatalf("load %s: %v", name, err)
	}
	return m
}

// findingsForRule returns only the findings whose RuleID matches.
func findingsForRule(findings []Finding, ruleID string) []Finding {
	out := []Finding{}
	for _, f := range findings {
		if f.RuleID == ruleID {
			out = append(out, f)
		}
	}
	return out
}

// TestCleanManifestProducesNoFindings is the baseline. If any rule
// fires here, that rule is a candidate false positive worth tightening
// before review.
func TestCleanManifestProducesNoFindings(t *testing.T) {
	m := loadFixture(t, "clean.yaml")
	got := Lint(m, LintOptions{})
	if len(got) != 0 {
		t.Errorf("expected zero findings on clean fixture, got %d:\n%s", len(got), formatFindings(got))
	}
}

// rule-by-rule positive cases. Each case loads a fixture engineered to
// trip exactly that rule, asserts the rule fires, and asserts the
// finding's Field path is the one we documented.
func TestEachRuleFiresOnItsFixture(t *testing.T) {
	cases := []struct {
		ruleID string
		file   string
		field  string // expected Finding.Field prefix
	}{
		{"AEGIS001", "aegis001-fs-read-system-root.yaml", "tools.filesystem.read[0]"},
		{"AEGIS002", "aegis002-fs-write-system-root.yaml", "tools.filesystem.write[0]"},
		{"AEGIS003", "aegis003-exec-bare-basename.yaml", "exec_grants[0].program"},
		{"AEGIS004", "aegis004-outbound-allow.yaml", "tools.network.outbound"},
		{"AEGIS005", "aegis005-write-grant-directory.yaml", "write_grants[0].resource"},
		{"AEGIS006", "aegis006-write-grant-eternal.yaml", "write_grants[0]"},
		{"AEGIS007", "aegis007-write-without-approval.yaml", "tools.filesystem.write"},
		{"AEGIS008", "aegis008-mcp-empty-allowed-tools.yaml", "tools.mcp[0].allowed_tools"},
		{"AEGIS009", "aegis009-approval-authorities-empty.yaml", "approval_authorities"},
		{"AEGIS010", "aegis010-name-mismatch.yaml", "agent.name"},
	}
	for _, tc := range cases {
		tc := tc
		t.Run(tc.ruleID, func(t *testing.T) {
			m := loadFixture(t, tc.file)
			findings := Lint(m, LintOptions{})
			rule := findingsForRule(findings, tc.ruleID)
			if len(rule) == 0 {
				t.Fatalf("rule %s did not fire on %s; got findings:\n%s",
					tc.ruleID, tc.file, formatFindings(findings))
			}
			if rule[0].Field != tc.field {
				t.Errorf("rule %s field: got %q want %q", tc.ruleID, rule[0].Field, tc.field)
			}
			if rule[0].Rationale == "" {
				t.Errorf("rule %s missing Rationale", tc.ruleID)
			}
		})
	}
}

// TestRuleSetCount pins the rule count so adding/removing a rule
// requires a test bump — keeps tests honest about rule churn.
func TestRuleSetCount(t *testing.T) {
	got := len(Rules())
	if got != 10 {
		t.Errorf("rule count: got %d want 10 (update this test if intentional)", got)
	}
}

// TestRuleSetIDsAreUnique asserts every rule has a unique AEGIS<NNN> ID.
func TestRuleSetIDsAreUnique(t *testing.T) {
	seen := map[string]bool{}
	for _, r := range Rules() {
		if seen[r.ID] {
			t.Errorf("duplicate rule ID %q", r.ID)
		}
		seen[r.ID] = true
		if !strings.HasPrefix(r.ID, "AEGIS") {
			t.Errorf("rule %q must use AEGIS<NNN> ID format", r.ID)
		}
	}
}

// TestSortStability — Lint must produce findings in a stable order so
// CI annotations diff cleanly across runs.
func TestSortStability(t *testing.T) {
	m := loadFixture(t, "aegis001-fs-read-system-root.yaml")
	first := Lint(m, LintOptions{})
	for i := 0; i < 10; i++ {
		again := Lint(m, LintOptions{})
		if !findingsEqual(first, again) {
			t.Fatalf("Lint output not stable across runs:\nfirst:\n%s\nagain:\n%s",
				formatFindings(first), formatFindings(again))
		}
	}
	// And the documented order is (Field, RuleID).
	if !sort.SliceIsSorted(first, func(i, j int) bool {
		if first[i].Field != first[j].Field {
			return first[i].Field < first[j].Field
		}
		return first[i].RuleID < first[j].RuleID
	}) {
		t.Errorf("findings not sorted by (Field, RuleID): %s", formatFindings(first))
	}
}

// TestSeverityOverrideElevatesWarn confirms an operator can elevate a
// warn into an error via LintOptions.SeverityOverride.
func TestSeverityOverrideElevatesWarn(t *testing.T) {
	m := loadFixture(t, "aegis006-write-grant-eternal.yaml")
	defaultRun := Lint(m, LintOptions{})
	if len(defaultRun) == 0 || defaultRun[0].Severity != SeverityWarn {
		t.Fatalf("expected default Warn for AEGIS006, got %s", formatFindings(defaultRun))
	}
	override := LintOptions{
		SeverityOverride: map[string]Severity{"AEGIS006": SeverityError},
	}
	overridden := Lint(m, override)
	if len(overridden) == 0 || overridden[0].Severity != SeverityError {
		t.Fatalf("override did not elevate AEGIS006 to Error: %s", formatFindings(overridden))
	}
	if !HasErrors(overridden) {
		t.Error("HasErrors should be true after override")
	}
}

// TestHasErrorsRespectsSeverity confirms HasErrors only counts errors.
func TestHasErrorsRespectsSeverity(t *testing.T) {
	m := loadFixture(t, "aegis006-write-grant-eternal.yaml") // default Warn
	if HasErrors(Lint(m, LintOptions{})) {
		t.Error("HasErrors should be false when no findings are SeverityError")
	}
}

func formatFindings(findings []Finding) string {
	var b strings.Builder
	for _, f := range findings {
		b.WriteString("  [")
		b.WriteString(f.SeverityName())
		b.WriteString("] ")
		b.WriteString(f.RuleID)
		b.WriteString(" @ ")
		b.WriteString(f.Field)
		b.WriteString(": ")
		b.WriteString(f.Message)
		b.WriteString("\n")
	}
	return b.String()
}

func findingsEqual(a, b []Finding) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i].RuleID != b[i].RuleID || a[i].Field != b[i].Field || a[i].Message != b[i].Message {
			return false
		}
	}
	return true
}
