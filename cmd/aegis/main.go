// Aegis-Node CLI — control-plane entry point.
//
// Per ADR-002 (Split-Language Architecture). The Go binary owns
// control-plane operations: linting manifests pre-deploy (`aegis
// validate` per F10 / ADR-012), and — once the operator scaffold
// matures — Kubernetes CRD reconciliation. Runtime / per-tool-call
// mediation lives in the Rust `aegis` binary (crates/cli).
//
// Subcommand dispatch is hand-rolled around flag.NewFlagSet rather
// than pulled-in cobra/urfave to keep the Go-side dep set lean and the
// supply-chain surface narrow per ADR-013's spirit.
package main

import (
	"fmt"
	"io"
	"os"

	"github.com/tosin2013/aegis-node/pkg/version"
)

func main() {
	if err := dispatch(os.Args[1:], os.Stdout, os.Stderr); err != nil {
		if _, ok := err.(errExit); !ok {
			fmt.Fprintln(os.Stderr, err)
		}
		os.Exit(exitCode(err))
	}
}

// dispatch routes argv to the subcommand handler. Extracted from main
// so tests can drive the CLI without process boundaries.
func dispatch(args []string, stdout, stderr io.Writer) error {
	if len(args) == 0 {
		printUsage(stderr)
		return errExit(2)
	}
	switch args[0] {
	case "validate":
		return runValidate(args[1:], stdout, stderr)
	case "version", "--version", "-V":
		fmt.Fprintf(stdout, "aegis %s\n", version.Version)
		return nil
	case "help", "--help", "-h":
		printUsage(stdout)
		return nil
	default:
		fmt.Fprintf(stderr, "unknown subcommand %q\n\n", args[0])
		printUsage(stderr)
		return errExit(2)
	}
}

func printUsage(w io.Writer) {
	fmt.Fprint(w, `aegis — Aegis-Node control-plane CLI

Usage:
  aegis <subcommand> [flags]

Subcommands:
  validate <manifest.yaml>    Lint a Permission Manifest pre-deploy (F10)
  version                     Print the build version
  help                        Show this message

For per-subcommand flags:
  aegis validate --help
`)
}

// errExit carries an exit code through error returns. main translates
// it to os.Exit; tests inspect it directly.
type errExit int

func (e errExit) Error() string { return fmt.Sprintf("exit %d", int(e)) }

func exitCode(err error) int {
	if err == nil {
		return 0
	}
	if e, ok := err.(errExit); ok {
		return int(e)
	}
	return 1
}
