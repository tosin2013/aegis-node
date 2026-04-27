// Aegis-Node CLI — control-plane entry point.
//
// Per ADR-002 (Split-Language Architecture). Phase 0 scaffold.
package main

import (
	"fmt"
	"os"

	"github.com/tosin2013/aegis-node/pkg/version"
)

func main() {
	fmt.Fprintf(os.Stdout, "aegis %s — Phase 0 scaffold\n", version.Version)
}
