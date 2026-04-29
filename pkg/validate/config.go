package validate

import (
	"fmt"
	"os"

	"gopkg.in/yaml.v3"
)

// ConfigFile is the on-disk shape of `.aegis-validate.yaml`.
//
// Operators commit this file at the repo root (or pass `--config <path>`
// to `aegis validate`) to elevate warnings into errors per their org's
// review bar — or, less commonly, to downgrade an error.
//
// Example:
//
//	# .aegis-validate.yaml
//	severity:
//	  AEGIS006: error  # eternal write_grants are unacceptable here
//	  AEGIS010: warn   # name-mismatch is a real bug, not just info
type ConfigFile struct {
	Severity map[string]string `yaml:"severity,omitempty"`
}

// LoadConfig parses path into a ConfigFile and converts it to
// LintOptions ready to pass to Lint. Returns an empty LintOptions{} on
// "file not found" so the no-config-file case Just Works.
func LoadConfig(path string) (LintOptions, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return LintOptions{}, nil
		}
		return LintOptions{}, fmt.Errorf("read %s: %w", path, err)
	}
	var cf ConfigFile
	if err := yaml.Unmarshal(data, &cf); err != nil {
		return LintOptions{}, fmt.Errorf("parse %s: %w", path, err)
	}
	if len(cf.Severity) == 0 {
		return LintOptions{}, nil
	}
	override := make(map[string]Severity, len(cf.Severity))
	known := knownRuleIDs()
	for ruleID, sev := range cf.Severity {
		if !known[ruleID] {
			return LintOptions{}, fmt.Errorf("%s: unknown rule %q", path, ruleID)
		}
		s, err := parseSeverity(sev)
		if err != nil {
			return LintOptions{}, fmt.Errorf("%s: rule %s: %w", path, ruleID, err)
		}
		override[ruleID] = s
	}
	return LintOptions{SeverityOverride: override}, nil
}

func parseSeverity(s string) (Severity, error) {
	switch s {
	case "info":
		return SeverityInfo, nil
	case "warn", "warning":
		return SeverityWarn, nil
	case "error":
		return SeverityError, nil
	default:
		return 0, fmt.Errorf("unknown severity %q (want info|warn|error)", s)
	}
}

func knownRuleIDs() map[string]bool {
	m := make(map[string]bool, len(registry()))
	for _, r := range registry() {
		m[r.ID] = true
	}
	return m
}
