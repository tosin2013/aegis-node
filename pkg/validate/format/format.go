// Package format renders []validate.Finding into one of the four
// output formats `aegis validate` supports: text (default human-
// readable), github (GitHub Actions annotations), junit (JUnit XML
// for enterprise CI), and json (newline-delimited records, one per
// Finding).
//
// Per F10-B / ADR-012 / issue #60. The schema for the JSON format
// is committed under schemas/validate/findings.schema.json.
//
// Every formatter sorts input by (file, line, col, RuleID) before
// emitting so output diffs cleanly across runs even when callers
// pre-sort differently. Today line/col are zero (the YAML AST hookup
// is a follow-up); the sort key still produces stable output.
package format

import (
	"encoding/json"
	"encoding/xml"
	"fmt"
	"io"
	"sort"
	"strings"

	"github.com/tosin2013/aegis-node/pkg/validate"
)

// Format identifies an output format. Use Parse to convert from a
// CLI flag value.
type Format string

const (
	FormatText   Format = "text"
	FormatGitHub Format = "github"
	FormatJUnit  Format = "junit"
	FormatJSON   Format = "json"
)

// Parse converts a string flag value into a Format, returning an
// explicit list of valid choices on failure.
func Parse(s string) (Format, error) {
	switch s {
	case "text", "github", "junit", "json":
		return Format(s), nil
	default:
		return "", fmt.Errorf("unknown format %q (want text|github|junit|json)", s)
	}
}

// Render writes findings to w using the named format. Caller-provided
// `file` is the manifest path used in github / junit / json output;
// pass "" if not applicable.
func Render(w io.Writer, file string, findings []validate.Finding, fmtKind Format) error {
	sorted := make([]validate.Finding, len(findings))
	copy(sorted, findings)
	sort.SliceStable(sorted, func(i, j int) bool {
		if sorted[i].Field != sorted[j].Field {
			return sorted[i].Field < sorted[j].Field
		}
		return sorted[i].RuleID < sorted[j].RuleID
	})
	switch fmtKind {
	case FormatText:
		return renderText(w, file, sorted)
	case FormatGitHub:
		return renderGitHub(w, file, sorted)
	case FormatJUnit:
		return renderJUnit(w, file, sorted)
	case FormatJSON:
		return renderJSON(w, file, sorted)
	default:
		return fmt.Errorf("unknown format %q", fmtKind)
	}
}

// ---------------------------------------------------------------------------
// text
// ---------------------------------------------------------------------------

func renderText(w io.Writer, file string, findings []validate.Finding) error {
	if len(findings) == 0 {
		_, err := fmt.Fprintf(w, "%s: clean — 0 findings\n", filenameOr(file, "<stdin>"))
		return err
	}
	for _, f := range findings {
		// `path:line:col: severity rule message`. Line/col are 0 until
		// the YAML AST hookup lands; the slot is reserved.
		if _, err := fmt.Fprintf(w, "%s:0:0: %s %s %s — %s\n",
			filenameOr(file, "<stdin>"),
			f.SeverityName(),
			f.RuleID,
			f.Field,
			f.Message); err != nil {
			return err
		}
	}
	errs, warns, infos := tally(findings)
	_, err := fmt.Fprintf(w, "\n%d error(s), %d warning(s), %d info\n", errs, warns, infos)
	return err
}

// ---------------------------------------------------------------------------
// github (Actions annotations)
// ---------------------------------------------------------------------------

func renderGitHub(w io.Writer, file string, findings []validate.Finding) error {
	for _, f := range findings {
		level := "notice"
		switch f.Severity {
		case validate.SeverityError:
			level = "error"
		case validate.SeverityWarn:
			level = "warning"
		}
		// Per https://docs.github.com/en/actions/learn-github-actions/workflow-commands-for-github-actions
		// ::level file=path,line=N,col=N,title=...::message
		// Newlines in message must be %0A-encoded.
		title := fmt.Sprintf("%s %s", f.RuleID, f.Field)
		msg := encodeGHA(f.Message)
		if _, err := fmt.Fprintf(w, "::%s file=%s,line=1,col=1,title=%s::%s\n",
			level, file, encodeGHA(title), msg); err != nil {
			return err
		}
	}
	return nil
}

// encodeGHA escapes characters that have meaning in workflow commands.
func encodeGHA(s string) string {
	r := strings.NewReplacer(
		"%", "%25",
		"\r", "%0D",
		"\n", "%0A",
		":", "%3A",
		",", "%2C",
	)
	return r.Replace(s)
}

// ---------------------------------------------------------------------------
// junit XML
// ---------------------------------------------------------------------------

type junitTestSuite struct {
	XMLName  xml.Name        `xml:"testsuite"`
	Name     string          `xml:"name,attr"`
	Tests    int             `xml:"tests,attr"`
	Failures int             `xml:"failures,attr"`
	Errors   int             `xml:"errors,attr"`
	Cases    []junitTestCase `xml:"testcase"`
}

type junitTestCase struct {
	Name      string        `xml:"name,attr"`
	ClassName string        `xml:"classname,attr"`
	Failure   *junitFailure `xml:"failure,omitempty"`
}

type junitFailure struct {
	Message string `xml:"message,attr"`
	Type    string `xml:"type,attr"`
	Text    string `xml:",chardata"`
}

func renderJUnit(w io.Writer, file string, findings []validate.Finding) error {
	suite := junitTestSuite{
		Name: "aegis-validate",
	}
	// Each finding is a failed testcase. Clean manifest is one passing
	// testcase so JUnit consumers see "1 test, 0 failures" rather than
	// "0 tests" (which some dashboards interpret as missing).
	if len(findings) == 0 {
		suite.Tests = 1
		suite.Cases = append(suite.Cases, junitTestCase{
			Name:      "clean",
			ClassName: filenameOr(file, "<stdin>"),
		})
	} else {
		for _, f := range findings {
			tc := junitTestCase{
				Name:      fmt.Sprintf("%s @ %s", f.RuleID, f.Field),
				ClassName: filenameOr(file, "<stdin>"),
			}
			if f.Severity == validate.SeverityError || f.Severity == validate.SeverityWarn {
				tc.Failure = &junitFailure{
					Message: f.Message,
					Type:    f.SeverityName(),
					Text:    f.Rationale,
				}
				if f.Severity == validate.SeverityError {
					suite.Errors++
				} else {
					suite.Failures++
				}
			}
			suite.Cases = append(suite.Cases, tc)
			suite.Tests++
		}
	}
	if _, err := io.WriteString(w, xml.Header); err != nil {
		return err
	}
	enc := xml.NewEncoder(w)
	enc.Indent("", "  ")
	if err := enc.Encode(suite); err != nil {
		return err
	}
	_, err := io.WriteString(w, "\n")
	return err
}

// ---------------------------------------------------------------------------
// json
// ---------------------------------------------------------------------------

// jsonRecord is the on-disk representation of one Finding. The JSON
// schema for this type lives at schemas/validate/findings.schema.json
// so consumers can build typed tooling (SIEM rules, CI dashboards).
type jsonRecord struct {
	File      string `json:"file,omitempty"`
	RuleID    string `json:"rule_id"`
	Severity  string `json:"severity"`
	Field     string `json:"field"`
	Message   string `json:"message"`
	Rationale string `json:"rationale,omitempty"`
}

func renderJSON(w io.Writer, file string, findings []validate.Finding) error {
	enc := json.NewEncoder(w)
	enc.SetEscapeHTML(false)
	for _, f := range findings {
		rec := jsonRecord{
			File:      file,
			RuleID:    f.RuleID,
			Severity:  f.SeverityName(),
			Field:     f.Field,
			Message:   f.Message,
			Rationale: f.Rationale,
		}
		if err := enc.Encode(rec); err != nil {
			return err
		}
	}
	return nil
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

func tally(findings []validate.Finding) (errs, warns, infos int) {
	for _, f := range findings {
		switch f.Severity {
		case validate.SeverityError:
			errs++
		case validate.SeverityWarn:
			warns++
		case validate.SeverityInfo:
			infos++
		}
	}
	return
}

func filenameOr(s, fallback string) string {
	if s == "" {
		return fallback
	}
	return s
}
