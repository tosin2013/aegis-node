package validate

import (
	"os"
	"path/filepath"
	"testing"
)

func TestLoadConfigMissingFileIsOK(t *testing.T) {
	opts, err := LoadConfig(filepath.Join(t.TempDir(), "absent.yaml"))
	if err != nil {
		t.Fatalf("missing file should be tolerated, got: %v", err)
	}
	if len(opts.SeverityOverride) != 0 {
		t.Errorf("missing file should yield empty overrides, got %v", opts.SeverityOverride)
	}
}

func TestLoadConfigParsesOverrides(t *testing.T) {
	path := writeYAML(t, `severity:
  AEGIS001: warn
  AEGIS006: error
`)
	opts, err := LoadConfig(path)
	if err != nil {
		t.Fatalf("LoadConfig: %v", err)
	}
	if got := opts.SeverityOverride["AEGIS001"]; got != SeverityWarn {
		t.Errorf("AEGIS001: got %v want Warn", got)
	}
	if got := opts.SeverityOverride["AEGIS006"]; got != SeverityError {
		t.Errorf("AEGIS006: got %v want Error", got)
	}
}

func TestLoadConfigRejectsUnknownRule(t *testing.T) {
	path := writeYAML(t, `severity:
  AEGIS999: error
`)
	if _, err := LoadConfig(path); err == nil {
		t.Fatal("expected error for unknown rule")
	}
}

func TestLoadConfigRejectsBadSeverity(t *testing.T) {
	path := writeYAML(t, `severity:
  AEGIS001: critical
`)
	if _, err := LoadConfig(path); err == nil {
		t.Fatal("expected error for unknown severity")
	}
}

func writeYAML(t *testing.T, body string) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), ".aegis-validate.yaml")
	if err := os.WriteFile(path, []byte(body), 0o600); err != nil {
		t.Fatalf("write: %v", err)
	}
	return path
}
