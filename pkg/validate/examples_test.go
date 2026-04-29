package validate

import (
	"path/filepath"
	"testing"

	"github.com/tosin2013/aegis-node/pkg/manifest"
)

// TestShippedExampleManifestsLintCleanly runs the linter against every
// committed example under schemas/manifest/v1/examples/ and asserts no
// finding is SeverityError. Warnings are tolerated (an example may
// deliberately demonstrate a less-defensible policy that still validates).
//
// This is a regression guard: a future rule that fires on a shipped
// example without that example being a deliberate counter-example is
// almost certainly a false positive.
func TestShippedExampleManifestsLintCleanly(t *testing.T) {
	matches, err := filepath.Glob("../../schemas/manifest/v1/examples/*.manifest.yaml")
	if err != nil {
		t.Fatalf("glob: %v", err)
	}
	if len(matches) == 0 {
		t.Fatal("no example manifests found; testdata path drifted?")
	}
	for _, p := range matches {
		p := p
		t.Run(filepath.Base(p), func(t *testing.T) {
			m, err := manifest.Load(p)
			if err != nil {
				t.Fatalf("load: %v", err)
			}
			findings := Lint(m, LintOptions{})
			for _, f := range findings {
				if f.Severity == SeverityError {
					t.Errorf("[%s] %s @ %s: %s", f.SeverityName(), f.RuleID, f.Field, f.Message)
				}
			}
		})
	}
}
