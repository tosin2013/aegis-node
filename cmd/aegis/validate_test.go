package main

import (
	"bytes"
	"path/filepath"
	"strings"
	"testing"
)

// TestDispatchUsage covers the no-arg / help / version paths.
func TestDispatchUsage(t *testing.T) {
	cases := []struct {
		name     string
		args     []string
		wantExit int
		stdout   string
	}{
		{"no args", []string{}, 2, ""},
		{"help long", []string{"--help"}, 0, "Usage:"},
		{"help short", []string{"help"}, 0, "Usage:"},
		{"version", []string{"version"}, 0, "aegis "},
		{"unknown subcommand", []string{"frobulate"}, 2, ""},
	}
	for _, tc := range cases {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			var stdout, stderr bytes.Buffer
			err := dispatch(tc.args, &stdout, &stderr)
			if exitCode(err) != tc.wantExit {
				t.Errorf("exit: got %d want %d (err=%v, stderr=%q)",
					exitCode(err), tc.wantExit, err, stderr.String())
			}
			if tc.stdout != "" && !strings.Contains(stdout.String(), tc.stdout) {
				t.Errorf("stdout missing %q: %q", tc.stdout, stdout.String())
			}
		})
	}
}

func TestValidateCleanExample(t *testing.T) {
	example := filepath.Join("..", "..", "schemas", "manifest", "v1", "examples",
		"agent-with-mcp.manifest.yaml")
	var stdout, stderr bytes.Buffer
	err := dispatch([]string{"validate", example}, &stdout, &stderr)
	if exitCode(err) != 0 {
		t.Errorf("clean example should exit 0, got %d (stderr=%q)", exitCode(err), stderr.String())
	}
	if !strings.Contains(stdout.String(), "clean — 0 findings") {
		t.Errorf("clean output missing 'clean — 0 findings':\n%s", stdout.String())
	}
}

func TestValidateTripFixtureExitsOne(t *testing.T) {
	fixture := filepath.Join("..", "..", "pkg", "validate", "testdata",
		"aegis001-fs-read-system-root.yaml")
	var stdout, stderr bytes.Buffer
	err := dispatch([]string{"validate", fixture}, &stdout, &stderr)
	if exitCode(err) != 1 {
		t.Errorf("error fixture should exit 1, got %d", exitCode(err))
	}
	if !strings.Contains(stdout.String(), "AEGIS001") {
		t.Errorf("output missing rule ID:\n%s", stdout.String())
	}
}

func TestValidateRejectsUnknownFormat(t *testing.T) {
	example := filepath.Join("..", "..", "schemas", "manifest", "v1", "examples",
		"read-only-research.manifest.yaml")
	var stdout, stderr bytes.Buffer
	err := dispatch([]string{"validate", "--format=yaml", example}, &stdout, &stderr)
	if exitCode(err) != 2 {
		t.Errorf("bad format flag should exit 2, got %d", exitCode(err))
	}
}

func TestValidateListRules(t *testing.T) {
	var stdout, stderr bytes.Buffer
	err := dispatch([]string{"validate", "--list-rules"}, &stdout, &stderr)
	if exitCode(err) != 0 {
		t.Errorf("--list-rules should exit 0, got %d (stderr=%q)", exitCode(err), stderr.String())
	}
	for _, want := range []string{"AEGIS001", "AEGIS010", "rationale:"} {
		if !strings.Contains(stdout.String(), want) {
			t.Errorf("--list-rules output missing %q", want)
		}
	}
}
