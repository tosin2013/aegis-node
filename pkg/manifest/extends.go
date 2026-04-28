package manifest

import (
	"fmt"
	"path/filepath"
	"strings"
)

// Resolved is a child manifest plus the resolved chain of parents
// (`extends:` walked transitively). Narrowing checks have already passed
// for every adjacent (child, parent) pair in the chain. The Parents slice
// is closest-first: Parents[0] is the direct parent, Parents[1] its
// grandparent, and so on.
type Resolved struct {
	Child   *Manifest
	Path    string
	Parents []*ResolvedParent
}

type ResolvedParent struct {
	Manifest *Manifest
	Path     string
}

// LoadResolved loads `path`, follows every `extends:` reference (DFS,
// closest parent first), validates each ancestor against the schema, and
// runs the narrowing check at every adjacent layer. Returns a *NarrowingError
// (wrapped where useful) on the first widening seen.
func LoadResolved(path string) (*Resolved, error) {
	visiting := make(map[string]bool)
	return loadResolvedRec(path, visiting, nil)
}

func loadResolvedRec(path string, visiting map[string]bool, chain []string) (*Resolved, error) {
	abs, err := filepath.Abs(path)
	if err != nil {
		return nil, fmt.Errorf("manifest: abs path for %s: %w", path, err)
	}
	if visiting[abs] {
		return nil, &CycleError{Chain: append(chain, abs)}
	}
	visiting[abs] = true
	defer delete(visiting, abs)

	child, err := Load(path)
	if err != nil {
		return nil, err
	}

	r := &Resolved{Child: child, Path: path}

	for _, ref := range child.Extends {
		parentPath := ResolveExtendsPath(path, ref)
		parentRes, err := loadResolvedRec(parentPath, visiting, append(chain, abs))
		if err != nil {
			return nil, err
		}
		if nErr := narrowsParent(child, parentRes.Child, path, parentPath); nErr != nil {
			return nil, nErr
		}
		r.Parents = append(r.Parents, &ResolvedParent{
			Manifest: parentRes.Child,
			Path:     parentPath,
		})
		// Flatten transitive ancestors so callers see the full chain.
		r.Parents = append(r.Parents, parentRes.Parents...)
	}
	return r, nil
}

// narrowsParent returns nil if `child` is a subset of `parent` for every
// permission category, or a *NarrowingError pointing at the first widening.
func narrowsParent(child, parent *Manifest, childPath, parentPath string) error {
	mk := func(field, msg string) *NarrowingError {
		return &NarrowingError{
			ChildPath:  childPath,
			ParentPath: parentPath,
			Field:      field,
			Message:    msg,
		}
	}

	if child.Tools.Filesystem != nil {
		var pRead, pWrite []string
		if parent.Tools.Filesystem != nil {
			pRead = parent.Tools.Filesystem.Read
			pWrite = parent.Tools.Filesystem.Write
		}
		for _, c := range child.Tools.Filesystem.Read {
			if !pathCovered(c, pRead) {
				return mk("tools.filesystem.read",
					fmt.Sprintf("path %q is not at-or-under any parent read path %v", c, pRead))
			}
		}
		for _, c := range child.Tools.Filesystem.Write {
			if !pathCovered(c, pWrite) {
				return mk("tools.filesystem.write",
					fmt.Sprintf("path %q is not at-or-under any parent write path %v", c, pWrite))
			}
		}
	}

	if child.Tools.Network != nil {
		pNet := parent.Tools.Network
		if err := networkSubset(child.Tools.Network.Outbound, parentPolicy(pNet, true)); err != nil {
			return mk("tools.network.outbound", err.Error())
		}
		if err := networkSubset(child.Tools.Network.Inbound, parentPolicy(pNet, false)); err != nil {
			return mk("tools.network.inbound", err.Error())
		}
	}

	for _, ca := range child.Tools.APIs {
		pa := findAPI(parent.Tools.APIs, ca.Name)
		if pa == nil {
			return mk(fmt.Sprintf("tools.apis[%s]", ca.Name),
				fmt.Sprintf("api %q is not granted by parent", ca.Name))
		}
		if !methodsSubset(ca.Methods, pa.Methods) {
			return mk(fmt.Sprintf("tools.apis[%s].methods", ca.Name),
				fmt.Sprintf("methods %v exceed parent methods %v", ca.Methods, pa.Methods))
		}
	}

	for _, cw := range child.WriteGrants {
		pw := findWriteGrant(parent.WriteGrants, cw.Resource)
		if pw == nil {
			return mk(fmt.Sprintf("write_grants[%s]", cw.Resource),
				fmt.Sprintf("resource %q is not granted for write by parent", cw.Resource))
		}
		if !stringSubset(cw.Actions, pw.Actions) {
			return mk(fmt.Sprintf("write_grants[%s].actions", cw.Resource),
				fmt.Sprintf("actions %v exceed parent actions %v", cw.Actions, pw.Actions))
		}
	}

	// exec_grants narrowing: every child program must exist in parent.
	// args_match: if parent has no args_match, child can have any (parent
	// is wider). If parent has args_match, child must have exactly the
	// same string (regex inclusion is undecidable in general; exact
	// match is the defensible v1 rule). Documented in the PR for #15.
	for _, ce := range child.ExecGrants {
		pe := findExecGrant(parent.ExecGrants, ce.Program)
		if pe == nil {
			return mk(fmt.Sprintf("exec_grants[%s]", ce.Program),
				fmt.Sprintf("program %q is not granted by parent", ce.Program))
		}
		if pe.ArgsMatch != "" && pe.ArgsMatch != ce.ArgsMatch {
			return mk(fmt.Sprintf("exec_grants[%s].args_match", ce.Program),
				fmt.Sprintf("parent restricts args_match to %q; child must match exactly (got %q)",
					pe.ArgsMatch, ce.ArgsMatch))
		}
	}

	// approval_required_for runs the *opposite* direction: child cannot
	// drop a class the parent insists on. Narrowing here means "at least
	// as strict", so child ⊇ parent.
	for _, pc := range parent.ApprovalRequiredFor {
		if !approvalContains(child.ApprovalRequiredFor, pc) {
			return mk("approval_required_for",
				fmt.Sprintf("parent requires approval class %q which child omits", pc))
		}
	}

	return nil
}

func findExecGrant(grants []ExecGrant, program string) *ExecGrant {
	for i := range grants {
		if grants[i].Program == program {
			return &grants[i]
		}
	}
	return nil
}

func parentPolicy(n *Network, outbound bool) *NetworkPolicy {
	if n == nil {
		return nil
	}
	if outbound {
		return n.Outbound
	}
	return n.Inbound
}

// pathCovered returns true if `c` is at-or-under any of `parents` with
// proper boundary handling: "/data" covers "/data" and "/data/x" but not
// "/data2". An empty parent list means the parent declared no paths and
// the child cannot widen by adding any.
func pathCovered(c string, parents []string) bool {
	for _, p := range parents {
		if p == c {
			return true
		}
		if p == "/" {
			return true
		}
		if strings.HasPrefix(c, p+"/") {
			return true
		}
	}
	return false
}

// networkSubset returns nil if `child` is no more permissive than `parent`.
// Ordering: deny < allowlist < allow. An allowlist child is a subset of an
// allowlist parent only if every child entry exact-matches a parent entry
// (host+port+protocol). Wildcards aren't supported in v1.
func networkSubset(child, parent *NetworkPolicy) error {
	if child == nil {
		return nil
	}
	pMode := NetworkDeny
	if parent != nil {
		pMode = parent.Mode
		if pMode == "" {
			pMode = NetworkDeny
		}
	}
	cMode := child.Mode
	if cMode == "" {
		cMode = NetworkDeny
	}

	switch pMode {
	case NetworkAllow:
		return nil // parent allows everything; child can be anything
	case NetworkDeny:
		if cMode != NetworkDeny {
			return fmt.Errorf("parent denies all but child sets %q", cMode)
		}
		return nil
	case NetworkAllowlist:
		switch cMode {
		case NetworkDeny:
			return nil
		case NetworkAllow:
			return fmt.Errorf("parent restricts to allowlist but child sets allow")
		case NetworkAllowlist:
			for _, e := range child.Allowlist {
				if !networkEntryIn(parent.Allowlist, e) {
					return fmt.Errorf("allowlist entry %+v not in parent allowlist", e)
				}
			}
			return nil
		}
	}
	return fmt.Errorf("unsupported policy mode %q", pMode)
}

func networkEntryIn(parents []NetworkAllowEntry, e NetworkAllowEntry) bool {
	for _, p := range parents {
		if p == e {
			return true
		}
	}
	return false
}

func findAPI(grants []APIGrant, name string) *APIGrant {
	for i := range grants {
		if grants[i].Name == name {
			return &grants[i]
		}
	}
	return nil
}

func findWriteGrant(grants []WriteGrant, resource string) *WriteGrant {
	for i := range grants {
		if grants[i].Resource == resource {
			return &grants[i]
		}
	}
	return nil
}

func methodsSubset(child, parent []string) bool {
	return stringSubset(child, parent)
}

func stringSubset(child, parent []string) bool {
	pset := make(map[string]struct{}, len(parent))
	for _, p := range parent {
		pset[p] = struct{}{}
	}
	for _, c := range child {
		if _, ok := pset[c]; !ok {
			return false
		}
	}
	return true
}

func approvalContains(classes []ApprovalClass, want ApprovalClass) bool {
	for _, c := range classes {
		if c == want {
			return true
		}
	}
	return false
}
