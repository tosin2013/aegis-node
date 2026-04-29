package main

import (
	"flag"
	"fmt"
	"io"

	"github.com/tosin2013/aegis-node/pkg/manifest"
	"github.com/tosin2013/aegis-node/pkg/validate"
	"github.com/tosin2013/aegis-node/pkg/validate/format"
)

// runValidate handles `aegis validate <manifest.yaml> [flags]`. Exit
// codes:
//
//	0  no findings, or only info/warn findings
//	1  one or more SeverityError findings (after config overrides)
//	2  usage error (bad flags, missing path, parse error)
//
// CI integrations consume the exit code; output is shaped by --format.
func runValidate(args []string, stdout, stderr io.Writer) error {
	fs := flag.NewFlagSet("validate", flag.ContinueOnError)
	fs.SetOutput(stderr)
	var (
		fmtFlag    = fs.String("format", "text", "Output format: text|github|junit|json")
		configPath = fs.String("config", ".aegis-validate.yaml",
			"Path to severity-override config (missing file is OK)")
		listRules = fs.Bool("list-rules", false,
			"Print every registered rule + default severity + rationale and exit")
	)
	fs.Usage = func() {
		fmt.Fprint(stderr, `aegis validate — lint a Permission Manifest

Usage:
  aegis validate [flags] <manifest.yaml>
  aegis validate --list-rules

Flags:
`)
		fs.PrintDefaults()
	}
	if err := fs.Parse(args); err != nil {
		// flag printed its own error; map to exit 2.
		return errExit(2)
	}

	if *listRules {
		printRules(stdout)
		return nil
	}

	if fs.NArg() == 0 {
		fs.Usage()
		return errExit(2)
	}
	if fs.NArg() > 1 {
		fmt.Fprintln(stderr, "validate accepts exactly one manifest path")
		return errExit(2)
	}

	out, err := format.Parse(*fmtFlag)
	if err != nil {
		fmt.Fprintln(stderr, err)
		return errExit(2)
	}

	path := fs.Arg(0)
	m, err := manifest.Load(path)
	if err != nil {
		fmt.Fprintf(stderr, "load %s: %v\n", path, err)
		return errExit(2)
	}

	opts, err := validate.LoadConfig(*configPath)
	if err != nil {
		fmt.Fprintln(stderr, err)
		return errExit(2)
	}

	findings := validate.Lint(m, opts)
	if err := format.Render(stdout, path, findings, out); err != nil {
		fmt.Fprintf(stderr, "render: %v\n", err)
		return errExit(1)
	}
	if validate.HasErrors(findings) {
		return errExit(1)
	}
	return nil
}

func printRules(w io.Writer) {
	rules := validate.Rules()
	for _, r := range rules {
		fmt.Fprintf(w, "%s [%s]\n", r.ID, defaultSeverityName(r.Default))
		fmt.Fprintf(w, "  %s\n", r.Summary)
		if r.Rationale != "" {
			fmt.Fprintf(w, "  rationale: %s\n", r.Rationale)
		}
		fmt.Fprintln(w)
	}
}

func defaultSeverityName(s validate.Severity) string {
	return s.String()
}
