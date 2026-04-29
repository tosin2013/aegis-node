// Package validate implements `aegis validate` — the Permission Manifest
// linter (F10 per ADR-012, sub-issue F10-A / #59).
//
// Schema validation already happens in pkg/manifest.Load via the embedded
// JSON Schema. This package layers semantic checks on top: rules that
// catch policies most likely to fail a security review (overly broad
// paths, eternal write_grants, unreachable approval channels, ...).
//
// Output formats (GitHub Actions annotations, JUnit XML, JSON, plain
// text) live in F10-B (#60); this package's API returns structured
// Finding values that the formatter consumes.
package validate

import (
	"fmt"
	"sort"

	"github.com/tosin2013/aegis-node/pkg/manifest"
)

// Severity classifies a lint Finding. Operators can override per-rule
// severity via LintOptions.SeverityOverride to elevate warnings to
// errors (or downgrade) for their org's review bar.
type Severity int

const (
	SeverityInfo Severity = iota
	SeverityWarn
	SeverityError
)

// String returns the lowercase name (info/warn/error) for output formatters.
func (s Severity) String() string {
	switch s {
	case SeverityInfo:
		return "info"
	case SeverityWarn:
		return "warn"
	case SeverityError:
		return "error"
	default:
		return fmt.Sprintf("severity(%d)", int(s))
	}
}

// Finding is one lint result. Field is a JSON-pointer-ish path into the
// manifest (e.g. "tools.filesystem.read[0]" or "write_grants[2].resource")
// so output formatters can produce file:line:col when paired with the
// raw YAML AST. The pkg/manifest schema validator owns line/column
// extraction; this package emits paths only.
type Finding struct {
	RuleID    string   `json:"rule_id"`
	Severity  Severity `json:"-"`
	Field     string   `json:"field"`
	Message   string   `json:"message"`
	Rationale string   `json:"rationale,omitempty"`
}

// SeverityName is the JSON form of Severity (for output formatters).
func (f Finding) SeverityName() string { return f.Severity.String() }

// Rule is one lint check. ID is the canonical AEGIS<NNN> identifier;
// Default is the severity used when LintOptions doesn't override it.
// Check returns zero or more Findings for a single manifest.
type Rule struct {
	ID        string
	Default   Severity
	Summary   string
	Rationale string
	Check     func(*manifest.Manifest) []Finding
}

// LintOptions configures one Lint pass. SeverityOverride maps rule IDs
// to a different severity than the rule's Default — operators set this
// from .aegis-validate.yaml so an org can raise warnings into errors
// without forking the rule set.
type LintOptions struct {
	SeverityOverride map[string]Severity
}

// Lint runs every registered rule against m, applies severity overrides
// from opts, and returns Findings sorted by (Field, RuleID) so output is
// stable across runs (required for diff-based CI annotations).
func Lint(m *manifest.Manifest, opts LintOptions) []Finding {
	all := make([]Finding, 0)
	for _, r := range registry() {
		findings := r.Check(m)
		for _, f := range findings {
			f.RuleID = r.ID
			if f.Rationale == "" {
				f.Rationale = r.Rationale
			}
			if sev, ok := opts.SeverityOverride[r.ID]; ok {
				f.Severity = sev
			} else {
				f.Severity = r.Default
			}
			all = append(all, f)
		}
	}
	sort.SliceStable(all, func(i, j int) bool {
		if all[i].Field != all[j].Field {
			return all[i].Field < all[j].Field
		}
		return all[i].RuleID < all[j].RuleID
	})
	return all
}

// HasErrors reports whether any Finding has SeverityError. Drives the
// CLI exit code in F10-B.
func HasErrors(findings []Finding) bool {
	for _, f := range findings {
		if f.Severity == SeverityError {
			return true
		}
	}
	return false
}

// Rules returns the registered rule set sorted by ID. Used by the CLI
// to print rule documentation (`aegis validate --list-rules`).
func Rules() []Rule {
	rs := registry()
	sort.SliceStable(rs, func(i, j int) bool { return rs[i].ID < rs[j].ID })
	return rs
}
