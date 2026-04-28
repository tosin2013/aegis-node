package manifest

import (
	"bytes"
	_ "embed"
	"fmt"
	"sync"

	"github.com/santhosh-tekuri/jsonschema/v5"
)

// schema_v1.json is a copy of schemas/manifest/v1/manifest.schema.json
// (go:embed cannot reach outside the package directory). The
// `TestEmbeddedSchemaMatchesCanonical` test asserts the two stay in lockstep;
// regenerate via `cp schemas/manifest/v1/manifest.schema.json pkg/manifest/schema_v1.json`
// when the canonical schema changes.
//
//go:embed schema_v1.json
var schemaBytes []byte

const schemaURL = "https://aegis-node.dev/schemas/manifest/v1.schema.json"

var (
	schemaOnce sync.Once
	schema     *jsonschema.Schema
	schemaErr  error
)

// loadSchema compiles the embedded v1 schema once.
func loadSchema() (*jsonschema.Schema, error) {
	schemaOnce.Do(func() {
		c := jsonschema.NewCompiler()
		c.Draft = jsonschema.Draft7
		if err := c.AddResource(schemaURL, bytes.NewReader(schemaBytes)); err != nil {
			schemaErr = fmt.Errorf("manifest: register embedded schema: %w", err)
			return
		}
		s, err := c.Compile(schemaURL)
		if err != nil {
			schemaErr = fmt.Errorf("manifest: compile embedded schema: %w", err)
			return
		}
		schema = s
	})
	return schema, schemaErr
}

// SchemaBytes returns the embedded JSON schema bytes — useful for cross-
// language conformance harnesses that need to validate against the same
// schema this package uses.
func SchemaBytes() []byte {
	out := make([]byte, len(schemaBytes))
	copy(out, schemaBytes)
	return out
}
