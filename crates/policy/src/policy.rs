//! Compiled enforcement state — answers `Allow / Deny / RequireApproval` for
//! a single I/O attempt without re-parsing the manifest each time.

use std::path::Path;

use chrono::{DateTime, Duration, Utc};

use crate::decision::{Decision, NetworkProto};
use crate::error::{Error, Result};
use crate::manifest::{
    ApprovalClass, Manifest, NetworkAllowEntry, NetworkMode, NetworkPolicy, NetworkProtocol,
    WriteAction, WriteGrant,
};

/// Closed-by-default policy engine. All `check_*` methods return `Deny`
/// when the manifest is silent — never inferred. Approval-class membership
/// upgrades an Allow to a RequireApproval.
#[derive(Debug, Clone)]
pub struct Policy {
    manifest: Manifest,
}

impl Policy {
    /// Load + parse a manifest YAML from disk. Refuses manifests that use
    /// `extends:` — Phase 1a expects pre-resolved input. Run the resolution
    /// in Go (`pkg/manifest`) and pass the resolved form here.
    pub fn from_yaml_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::from_yaml_bytes(&bytes)
    }

    pub fn from_yaml_bytes(bytes: &[u8]) -> Result<Self> {
        let manifest: Manifest = serde_yaml::from_slice(bytes)?;
        if !manifest.extends.is_empty() {
            return Err(Error::ExtendsUnsupported(manifest.extends.len()));
        }
        Ok(Self { manifest })
    }

    pub fn from_manifest(manifest: Manifest) -> Self {
        Self { manifest }
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Filesystem read: closed-by-default, allowed iff path is at-or-under
    /// some `tools.filesystem.read` entry.
    pub fn check_filesystem_read(&self, path: &Path) -> Decision {
        let allowed = self
            .manifest
            .tools
            .filesystem
            .as_ref()
            .map(|f| paths_cover(path, &f.read))
            .unwrap_or(false);
        if !allowed {
            return Decision::deny(format!(
                "filesystem read of {} not granted by manifest",
                path.display()
            ));
        }
        Decision::Allow
    }

    /// Filesystem write: closed-by-default. Allowed iff a matching write
    /// grant exists OR `tools.filesystem.write` covers the path. If the
    /// grant requires approval, returns `RequireApproval`; if the manifest
    /// also lists `any_write` in `approval_required_for`, same result.
    ///
    /// `now` and `session_start` are consulted against `write_grant.duration`
    /// and `expires_at` (per F7 / ADR-009): a grant is valid only if both
    /// time bounds (where present) are still in window. Grants with no
    /// time fields are treated as eternal (current behavior pre-#38).
    /// NTP skew during a session can cause early/late expiry — short
    /// sessions are the documented mitigation.
    pub fn check_filesystem_write(
        &self,
        path: &Path,
        now: DateTime<Utc>,
        session_start: DateTime<Utc>,
    ) -> Decision {
        if let Some(g) = self.find_write_grant(path, WriteAction::Write, now, session_start) {
            return self.write_decision(path, g, WriteAction::Write);
        }
        let covered = self
            .manifest
            .tools
            .filesystem
            .as_ref()
            .map(|f| paths_cover(path, &f.write))
            .unwrap_or(false);
        if covered {
            return self.upgrade_for_approval(
                Decision::Allow,
                ApprovalClass::AnyWrite,
                "any_write requires approval",
            );
        }
        Decision::deny(format!(
            "filesystem write of {} not granted by manifest",
            path.display()
        ))
    }

    /// Filesystem delete: only allowed via a write grant whose `actions`
    /// includes `delete`. `approval_required_for: [any_delete]` upgrades.
    /// Time-bounded enforcement matches `check_filesystem_write`.
    pub fn check_filesystem_delete(
        &self,
        path: &Path,
        now: DateTime<Utc>,
        session_start: DateTime<Utc>,
    ) -> Decision {
        if let Some(g) = self.find_write_grant(path, WriteAction::Delete, now, session_start) {
            return self.write_decision(path, g, WriteAction::Delete);
        }
        Decision::deny(format!(
            "filesystem delete of {} not granted by any write_grant",
            path.display()
        ))
    }

    /// Network outbound: closed-by-default. Per ADR-008 the absence of a
    /// network policy means deny. `any_network_outbound` upgrades to
    /// approval.
    pub fn check_network_outbound(
        &self,
        host: &str,
        port: u16,
        protocol: NetworkProto,
    ) -> Decision {
        let policy = self
            .manifest
            .tools
            .network
            .as_ref()
            .and_then(|n| n.outbound.as_ref());
        let base = network_decision(policy, host, port, protocol, "outbound");
        self.upgrade_for_approval(
            base,
            ApprovalClass::AnyNetworkOutbound,
            "any_network_outbound requires approval",
        )
    }

    pub fn check_network_inbound(&self, host: &str, port: u16, protocol: NetworkProto) -> Decision {
        let policy = self
            .manifest
            .tools
            .network
            .as_ref()
            .and_then(|n| n.inbound.as_ref());
        network_decision(policy, host, port, protocol, "inbound")
    }

    /// Exec: closed-by-default. A grant matches if its `program` field
    /// equals the query path (when the field has a slash) or the query's
    /// basename (when the field is a bare name). `any_exec` upgrades a
    /// hit to RequireApproval. `args_match` is stored but not enforced
    /// until the runtime can pass argv through.
    pub fn check_exec(&self, program: &Path) -> Decision {
        let matched = self
            .manifest
            .exec_grants
            .iter()
            .any(|g| program_matches(&g.program, program));
        if !matched {
            return Decision::deny(format!(
                "exec of {} not granted by manifest",
                program.display()
            ));
        }
        self.upgrade_for_approval(
            Decision::Allow,
            ApprovalClass::AnyExec,
            "any_exec requires approval",
        )
    }

    fn find_write_grant(
        &self,
        path: &Path,
        want: WriteAction,
        now: DateTime<Utc>,
        session_start: DateTime<Utc>,
    ) -> Option<&WriteGrant> {
        let p = path.to_string_lossy();
        self.manifest.write_grants.iter().find(|g| {
            g.resource == p
                && g.actions.contains(&want)
                && grant_time_valid(g, now, session_start)
        })
    }

    fn write_decision(&self, path: &Path, g: &WriteGrant, action: WriteAction) -> Decision {
        let class = match action {
            WriteAction::Delete => ApprovalClass::AnyDelete,
            _ => ApprovalClass::AnyWrite,
        };
        if g.approval_required || self.manifest.approval_required_for.contains(&class) {
            return Decision::approval(format!(
                "{:?} on {} requires approval per write_grant",
                action,
                path.display()
            ));
        }
        Decision::Allow
    }

    fn upgrade_for_approval(&self, base: Decision, class: ApprovalClass, reason: &str) -> Decision {
        match base {
            Decision::Allow if self.manifest.approval_required_for.contains(&class) => {
                Decision::approval(reason)
            }
            other => other,
        }
    }
}

/// True if a manifest's exec_grant `program` field matches the query.
/// Slash-bearing strings are treated as absolute paths; bare strings are
/// treated as basenames.
fn program_matches(grant_program: &str, query: &Path) -> bool {
    if grant_program.contains('/') {
        return Path::new(grant_program) == query;
    }
    query
        .file_name()
        .map(|f| f == grant_program)
        .unwrap_or(false)
}

/// Returns true if `path` is at-or-under any of `parents`. "/data" covers
/// "/data/x" and "/data" itself but not "/data2".
fn paths_cover(path: &Path, parents: &[String]) -> bool {
    let p = path.to_string_lossy();
    for parent in parents {
        if parent == p.as_ref() {
            return true;
        }
        if parent == "/" {
            return true;
        }
        let with_slash = format!("{parent}/");
        if p.starts_with(&with_slash) {
            return true;
        }
    }
    false
}

fn network_decision(
    policy: Option<&NetworkPolicy>,
    host: &str,
    port: u16,
    protocol: NetworkProto,
    direction: &str,
) -> Decision {
    let policy = match policy {
        Some(p) => p,
        None => {
            return Decision::deny(format!(
                "network {direction} denied: manifest has no policy"
            ));
        }
    };
    match policy {
        NetworkPolicy::Mode(NetworkMode::Allow) => Decision::Allow,
        NetworkPolicy::Mode(NetworkMode::Deny) => {
            Decision::deny(format!("network {direction} denied: manifest sets deny"))
        }
        NetworkPolicy::Allowlist { allowlist } => {
            if allowlist
                .iter()
                .any(|e| matches_allow_entry(e, host, port, protocol))
            {
                Decision::Allow
            } else {
                Decision::deny(format!(
                    "network {direction} {host}:{port} not in manifest allowlist"
                ))
            }
        }
    }
}

fn matches_allow_entry(
    e: &NetworkAllowEntry,
    host: &str,
    port: u16,
    protocol: NetworkProto,
) -> bool {
    if e.host != host {
        return false;
    }
    if let Some(p) = e.port {
        if p != port {
            return false;
        }
    }
    if let Some(proto) = e.protocol {
        if !proto_compatible(proto, protocol) {
            return false;
        }
    }
    true
}

fn proto_compatible(allowed: NetworkProtocol, actual: NetworkProto) -> bool {
    match (allowed, actual) {
        (_, NetworkProto::Any) => true,
        (NetworkProtocol::Http, NetworkProto::Http) => true,
        (NetworkProtocol::Https, NetworkProto::Https) => true,
        (NetworkProtocol::Tcp, NetworkProto::Tcp) => true,
        (NetworkProtocol::Udp, NetworkProto::Udp) => true,
        // HTTP/HTTPS are TCP under the hood; an allowlist entry that says
        // "https" should not match a callsite that says "tcp" because the
        // semantic intent differs.
        _ => false,
    }
}

/// Returns true iff the grant's time bounds (if any) are still in window.
/// `expires_at` is an absolute wall-clock cut-off (RFC 3339); `duration`
/// is relative to `session_start` (ISO-8601 form). Both must hold when
/// both are present (logical AND — most restrictive wins).
///
/// Malformed values are treated as invalid → grant is filtered out.
/// Closed-by-default semantics: an unparseable bound never accidentally
/// allows the operation.
fn grant_time_valid(
    grant: &WriteGrant,
    now: DateTime<Utc>,
    session_start: DateTime<Utc>,
) -> bool {
    if let Some(ref expires) = grant.expires_at {
        match expires.parse::<DateTime<Utc>>() {
            Ok(exp) => {
                if now >= exp {
                    return false;
                }
            }
            Err(_) => return false,
        }
    }
    if let Some(ref dur_str) = grant.duration {
        match parse_iso8601_duration(dur_str) {
            Some(dur) => {
                let elapsed = now - session_start;
                if elapsed >= dur {
                    return false;
                }
            }
            None => return false,
        }
    }
    true
}

/// Parse an ISO-8601 duration of the form `P[<n>D][T[<n>H][<n>M][<n>S]]`
/// into [`chrono::Duration`]. Integer values only; no fractional seconds,
/// weeks, months, or years (those rarely appear in audit policies and
/// add ambiguity around calendar arithmetic).
///
/// Examples that parse: `PT1H`, `PT30M`, `P1D`, `P1DT12H`, `PT45S`.
fn parse_iso8601_duration(s: &str) -> Option<Duration> {
    let s = s.strip_prefix('P')?;
    let (date_part, time_part) = match s.find('T') {
        Some(idx) => (&s[..idx], &s[idx + 1..]),
        None => (s, ""),
    };

    let mut total = Duration::zero();

    if !date_part.is_empty() {
        let n: i64 = date_part.strip_suffix('D')?.parse().ok()?;
        if n < 0 {
            return None;
        }
        total = total.checked_add(&Duration::days(n))?;
    }

    let mut t = time_part;
    while !t.is_empty() {
        let unit_pos = t.find(|c: char| matches!(c, 'H' | 'M' | 'S'))?;
        let n: i64 = t[..unit_pos].parse().ok()?;
        if n < 0 {
            return None;
        }
        let unit = t.as_bytes()[unit_pos];
        let increment = match unit {
            b'H' => Duration::hours(n),
            b'M' => Duration::minutes(n),
            b'S' => Duration::seconds(n),
            _ => return None,
        };
        total = total.checked_add(&increment)?;
        t = &t[unit_pos + 1..];
    }

    Some(total)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod time_tests {
    use super::*;

    #[test]
    fn iso8601_duration_examples() {
        assert_eq!(parse_iso8601_duration("PT1H"), Some(Duration::hours(1)));
        assert_eq!(parse_iso8601_duration("PT30M"), Some(Duration::minutes(30)));
        assert_eq!(parse_iso8601_duration("PT45S"), Some(Duration::seconds(45)));
        assert_eq!(parse_iso8601_duration("P1D"), Some(Duration::days(1)));
        assert_eq!(
            parse_iso8601_duration("P1DT12H"),
            Some(Duration::hours(36)),
        );
        assert_eq!(
            parse_iso8601_duration("PT1H30M"),
            Some(Duration::minutes(90)),
        );
        assert_eq!(parse_iso8601_duration("PT0S"), Some(Duration::zero()));

        assert!(parse_iso8601_duration("").is_none());
        assert!(parse_iso8601_duration("1H").is_none());
        assert!(parse_iso8601_duration("PT1X").is_none());
        assert!(parse_iso8601_duration("P1DT").is_none()); // T present but empty
    }
}
