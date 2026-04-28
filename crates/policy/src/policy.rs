//! Compiled enforcement state — answers `Allow / Deny / RequireApproval` for
//! a single I/O attempt without re-parsing the manifest each time.

use std::path::Path;

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
    pub fn check_filesystem_write(&self, path: &Path) -> Decision {
        if let Some(g) = self.find_write_grant(path, WriteAction::Write) {
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
    pub fn check_filesystem_delete(&self, path: &Path) -> Decision {
        if let Some(g) = self.find_write_grant(path, WriteAction::Delete) {
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

    /// Exec: v1 schema has no exec-grant primitive. Always denied. A future
    /// schema version will introduce explicit exec grants — this method
    /// will gain a corresponding manifest lookup at that point.
    pub fn check_exec(&self, program: &Path) -> Decision {
        let _ = program;
        Decision::deny(
            "exec is not grantable in manifest schema v1; future schema will add exec_grants"
                .to_string(),
        )
    }

    fn find_write_grant(&self, path: &Path, want: WriteAction) -> Option<&WriteGrant> {
        let p = path.to_string_lossy();
        self.manifest
            .write_grants
            .iter()
            .find(|g| g.resource == p && g.actions.contains(&want))
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
