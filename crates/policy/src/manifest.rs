//! Rust mirror of the Permission Manifest schema (v1).
//!
//! Single source of truth for the schema lives at
//! `schemas/manifest/v1/manifest.schema.json` and is enforced by the Go
//! validator (`pkg/manifest`). This module deserializes the same shape on
//! the Rust side so the runtime can compile a [`crate::Policy`] from disk.
//!
//! Phase 1a parses single-file manifests (no `extends:` resolution). Once
//! the control plane lands, the resolved manifest will arrive as JSON over
//! the gRPC channel and this module's loader becomes a fallback for local
//! dev/CLI use.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    pub agent: Agent,
    pub identity: Identity,
    #[serde(default)]
    pub extends: Vec<String>,
    pub tools: Tools,
    #[serde(default, rename = "write_grants")]
    pub write_grants: Vec<WriteGrant>,
    #[serde(default, rename = "approval_required_for")]
    pub approval_required_for: Vec<ApprovalClass>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    #[serde(rename = "spiffeId")]
    pub spiffe_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tools {
    #[serde(default)]
    pub filesystem: Option<Filesystem>,
    #[serde(default)]
    pub network: Option<Network>,
    #[serde(default)]
    pub apis: Vec<ApiGrant>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Filesystem {
    #[serde(default)]
    pub read: Vec<String>,
    #[serde(default)]
    pub write: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Network {
    #[serde(default)]
    pub outbound: Option<NetworkPolicy>,
    #[serde(default)]
    pub inbound: Option<NetworkPolicy>,
}

/// `oneOf {string, object}` from the schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NetworkPolicy {
    Mode(NetworkMode),
    Allowlist {
        allowlist: Vec<NetworkAllowEntry>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkMode {
    Deny,
    Allow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkAllowEntry {
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub protocol: Option<NetworkProtocol>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkProtocol {
    Http,
    Https,
    Tcp,
    Udp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiGrant {
    pub name: String,
    #[serde(default)]
    pub methods: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteGrant {
    pub resource: String,
    pub actions: Vec<WriteAction>,
    #[serde(default)]
    pub duration: Option<String>,
    #[serde(default, rename = "expires_at")]
    pub expires_at: Option<String>,
    #[serde(default, rename = "approval_required")]
    pub approval_required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteAction {
    Write,
    Delete,
    Update,
    Create,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalClass {
    AnyWrite,
    AnyDelete,
    AnyNetworkOutbound,
    AnyExec,
}
