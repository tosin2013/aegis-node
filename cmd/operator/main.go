// Aegis-Node Kubernetes Operator — Phase 2 entry point.
//
// Per ADR-015 (Three-Phase Deployment Roadmap). Phase 0 scaffold.
package main

import (
	"fmt"
	"os"

	"github.com/tosin2013/aegis-node/pkg/version"
)

func main() {
	fmt.Fprintf(os.Stdout, "aegis-operator %s — Phase 0 scaffold\n", version.Version)
}
