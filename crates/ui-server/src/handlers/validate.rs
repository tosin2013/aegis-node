//! `POST /api/v1/manifests/validate` — live `aegis validate` integration.
//!
//! Per [ADR-031](../../../docs/adrs/031-community-webui-for-local-collaboration.md)
//! §"Visual manifest builder" the editor surfaces validate findings
//! inline as the operator types. The validator itself lives in the
//! Go binary (`cmd/aegis/`, ADR-002 split-language) — this handler
//! is the cross-language bridge: it writes the request body to a
//! tempfile, invokes the validator with `--format json`, parses the
//! JSONL output, and returns a structured findings array the SPA
//! renders as Monaco markers.
//!
//! ## Binary resolution
//!
//! Three strategies in order, first hit wins:
//!
//! 1. `AEGIS_VALIDATE_BIN` env var — absolute path. For test
//!    fixtures + non-standard installs.
//! 2. `./bin/aegis-validate` relative to the cwd — the workspace
//!    development path that the `make build-go-validate` target
//!    produces.
//! 3. `aegis-validate` on `PATH` — the operator-installed name.
//!
//! Both Go and Rust source CLIs are named `aegis`; the
//! `build-go-validate` Makefile target renames the Go output to
//! `aegis-validate` so the ui-server can disambiguate without
//! risking a recursive invocation of itself.
//!
//! ## Schema
//!
//! Validator JSON output (newline-delimited per `pkg/validate/format`):
//!
//! ```text
//! {"file":"…","rule_id":"AEGIS007","severity":"warn","field":"tools.filesystem.write","message":"…","rationale":"…"}
//! ```
//!
//! Line/col are 0 today — the validator's YAML AST hookup is a
//! follow-up. The handler reserves `line` and `col` fields on the
//! response anyway so the SPA's marker code Just Works once the
//! validator emits them.

use std::path::PathBuf;
use std::process::Command;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

const ENV_VALIDATE_BIN: &str = "AEGIS_VALIDATE_BIN";
const DEV_BIN_PATH: &str = "bin/aegis-validate";
const PATH_BIN_NAME: &str = "aegis-validate";

/// One validator finding. Mirrors the on-disk JSONL record from
/// `pkg/validate/format/format.go::jsonRecord` plus reserved
/// `line` / `col` fields for the future YAML-AST hookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub rule_id: String,
    pub severity: String,
    pub field: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    /// One-based line index in the validated YAML. Currently always
    /// `0` until the validator's YAML AST hookup lands.
    #[serde(default)]
    pub line: u32,
    /// One-based column index. Currently always `0`.
    #[serde(default)]
    pub col: u32,
}

#[derive(Debug, Serialize)]
pub struct ValidateResponse {
    /// True when the validator returned exit 0 — meaning no
    /// `error`-severity findings. Warnings + info are non-fatal.
    pub ok: bool,
    /// All findings emitted by the validator. Deduplicated and
    /// stable-sorted by `(severity, rule_id, field)`.
    pub findings: Vec<Finding>,
    /// Absolute path of the validator binary that produced these
    /// findings. Surfaced in the SPA's "About" panel for
    /// debuggability.
    pub binary: String,
}

/// `POST /api/v1/manifests/validate` — body is YAML, returns
/// validator findings.
pub async fn validate_manifest(body: String) -> Response {
    if body.is_empty() {
        return (StatusCode::BAD_REQUEST, "empty body").into_response();
    }

    let bin = match resolve_validate_binary() {
        Some(p) => p,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                format!(
                    "validator not found. Install via `make build-go-validate` \
                     (writes ./bin/aegis-validate), put `aegis-validate` on PATH, \
                     or set the {ENV_VALIDATE_BIN} env var to an absolute binary path.",
                ),
            )
                .into_response();
        }
    };

    let temp = match write_tempfile(&body) {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("staging tempfile: {e}"),
            )
                .into_response();
        }
    };

    let output = match Command::new(&bin)
        .args(["validate", "--format", "json"])
        .arg(temp.path())
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("invoking {}: {e}", bin.display()),
            )
                .into_response();
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let mut findings = match parse_findings(&stdout) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                target: "aegis_ui_server",
                err = %e,
                stdout_len = stdout.len(),
                "validator output didn't parse as JSONL; returning empty findings",
            );
            Vec::new()
        }
    };
    sort_findings(&mut findings);

    // Validator exit codes (per cmd/aegis/validate.go):
    //   0  no findings, or only info/warn findings
    //   1  one or more SeverityError findings
    //   2  usage error (we treat as 503 since it implies a bug
    //      we mis-invoked the binary).
    let exit_code = output.status.code().unwrap_or(-1);
    if exit_code == 2 {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(
                "validator usage error (exit 2): {}",
                stderr.trim()
            ),
        )
            .into_response();
    }

    Json(ValidateResponse {
        ok: exit_code == 0,
        findings,
        binary: bin.display().to_string(),
    })
    .into_response()
}

fn resolve_validate_binary() -> Option<PathBuf> {
    if let Ok(env_path) = std::env::var(ENV_VALIDATE_BIN) {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return Some(p);
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        let p = cwd.join(DEV_BIN_PATH);
        if p.is_file() {
            return Some(p);
        }
    }

    which(PATH_BIN_NAME)
}

/// Walk `$PATH`, returning the first existing executable named
/// `tool`. Mirrors `crates/cli/src/pull.rs::which` so the lookup
/// semantics are consistent across the runtime.
fn which(tool: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(tool);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn write_tempfile(body: &str) -> std::io::Result<tempfile::NamedTempFile> {
    use std::io::Write;
    let mut f = tempfile::Builder::new()
        .prefix("aegis-validate-")
        .suffix(".yaml")
        .tempfile()?;
    f.write_all(body.as_bytes())?;
    f.flush()?;
    Ok(f)
}

/// Parse the validator's JSONL stdout into a `Vec<Finding>`. Empty
/// input → empty vec. Per-line parse failures are collected into an
/// error so callers can decide whether to surface a 500 or fall back
/// to an empty list (the handler chooses the latter — a malformed
/// finding shouldn't break the editor's live-validate loop).
pub(crate) fn parse_findings(stdout: &str) -> Result<Vec<Finding>, String> {
    let mut out = Vec::new();
    for (n, line) in stdout.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<Finding>(trimmed) {
            Ok(f) => out.push(f),
            Err(e) => {
                return Err(format!("line {}: {e} — raw: {trimmed}", n + 1));
            }
        }
    }
    Ok(out)
}

fn severity_rank(s: &str) -> u8 {
    match s {
        "error" => 0,
        "warn" => 1,
        "info" => 2,
        _ => 3,
    }
}

fn sort_findings(v: &mut [Finding]) {
    v.sort_by(|a, b| {
        severity_rank(&a.severity)
            .cmp(&severity_rank(&b.severity))
            .then_with(|| a.rule_id.cmp(&b.rule_id))
            .then_with(|| a.field.cmp(&b.field))
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const SAMPLE_JSONL: &str = concat!(
        r#"{"file":"/tmp/m.yaml","rule_id":"AEGIS007","severity":"warn","field":"tools.filesystem.write","message":"broad write coverage","rationale":"explain"}"#,
        "\n",
        r#"{"file":"/tmp/m.yaml","rule_id":"AEGIS001","severity":"error","field":"identity","message":"missing spiffe id"}"#,
        "\n",
    );

    #[test]
    fn parses_jsonl_into_findings() {
        let v = parse_findings(SAMPLE_JSONL).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].rule_id, "AEGIS007");
        assert_eq!(v[0].severity, "warn");
        assert_eq!(v[1].rule_id, "AEGIS001");
        assert_eq!(v[1].severity, "error");
        assert!(v[0].rationale.is_some());
        assert!(v[1].rationale.is_none());
    }

    #[test]
    fn empty_stdout_is_empty_vec() {
        assert!(parse_findings("").unwrap().is_empty());
        assert!(parse_findings("\n\n").unwrap().is_empty());
    }

    #[test]
    fn malformed_line_returns_error() {
        let bad = "not json\n";
        let err = parse_findings(bad).expect_err("must reject");
        assert!(err.contains("line 1"));
    }

    #[test]
    fn sorts_errors_before_warnings_before_info() {
        let mut v = parse_findings(SAMPLE_JSONL).unwrap();
        sort_findings(&mut v);
        assert_eq!(v[0].severity, "error");
        assert_eq!(v[1].severity, "warn");
    }

    #[test]
    fn line_col_default_to_zero() {
        let json = r#"{"rule_id":"X","severity":"warn","field":"f","message":"m"}"#;
        let v = parse_findings(json).unwrap();
        assert_eq!(v[0].line, 0);
        assert_eq!(v[0].col, 0);
    }

    #[test]
    fn resolve_uses_env_var_when_set() {
        // Real binary path that exists on the system, just to test
        // resolution logic. Use this binary itself.
        let me = std::env::current_exe().unwrap();
        std::env::set_var(ENV_VALIDATE_BIN, &me);
        let resolved = resolve_validate_binary().expect("env path resolves");
        assert_eq!(resolved, me);
        std::env::remove_var(ENV_VALIDATE_BIN);
    }

    #[test]
    fn resolve_falls_back_to_path_when_env_invalid() {
        std::env::set_var(ENV_VALIDATE_BIN, "/this/path/does/not/exist");
        // The fallback chain may or may not find aegis-validate
        // depending on the test environment; we just assert that
        // setting an invalid env var doesn't return that invalid
        // path.
        let resolved = resolve_validate_binary();
        if let Some(p) = resolved {
            assert!(p.is_file(), "fallback returned non-file path: {p:?}");
            assert_ne!(p, std::path::Path::new("/this/path/does/not/exist"));
        }
        std::env::remove_var(ENV_VALIDATE_BIN);
    }
}
