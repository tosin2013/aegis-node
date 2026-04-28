package manifest

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

const conformancePath = "../../tests/conformance/cases.json"

type conformanceFile struct {
	Version string            `json:"version"`
	Cases   []conformanceCase `json:"cases"`
}

type conformanceCase struct {
	Name     string       `json:"name"`
	Manifest string       `json:"manifest"`
	Query    Query        `json:"query"`
	Expected DecisionKind `json:"expected"`
}

func TestConformance_GoSide(t *testing.T) {
	data, err := os.ReadFile(conformancePath)
	if err != nil {
		t.Fatalf("read %s: %v", conformancePath, err)
	}
	var cf conformanceFile
	if err := json.Unmarshal(data, &cf); err != nil {
		t.Fatalf("parse cases.json: %v", err)
	}
	if cf.Version != "1" {
		t.Fatalf("unexpected fixture version %q", cf.Version)
	}
	if len(cf.Cases) == 0 {
		t.Fatal("no cases in fixture")
	}

	casesDir := filepath.Dir(conformancePath)
	for _, c := range cf.Cases {
		c := c
		t.Run(c.Name, func(t *testing.T) {
			manifestPath := filepath.Join(casesDir, c.Manifest)
			m, err := Load(manifestPath)
			if err != nil {
				t.Fatalf("load %s: %v", manifestPath, err)
			}
			got := m.Decide(c.Query)
			if got.Kind != c.Expected {
				t.Errorf("decision drift: want %q got %q (reason=%q)", c.Expected, got.Kind, got.Reason)
			}
		})
	}
}
