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
    #[serde(default, rename = "exec_grants")]
    pub exec_grants: Vec<ExecGrant>,
    /// SPIFFE IDs allowed to issue approvals over the F3 mTLS signed-API
    /// channel (ADR-005, ADR-003, issue #36). Empty/absent => mTLS
    /// approvals are refused outright.
    #[serde(default, rename = "approval_authorities")]
    pub approval_authorities: Vec<String>,
    /// Inference-time configuration (per ADR-014, LLM-C / issue #72).
    /// Additive — `None` means "backend defaults."
    #[serde(default)]
    pub inference: Option<Inference>,
}

/// Inference-time configuration block. Currently carries determinism
/// knobs only; future LLM- sub-issues may add more.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inference {
    #[serde(default)]
    pub determinism: Option<DeterminismKnobs>,
}

/// Sampling determinism knobs (LLM-C). All fields optional; absence
/// means "backend default for that knob." Setting `seed` and
/// `temperature: 0.0` together gets byte-identical output across runs
/// — the configuration auditors rely on for replay verification.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeterminismKnobs {
    #[serde(default)]
    pub seed: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub top_k: Option<u32>,
    #[serde(default)]
    pub repeat_penalty: Option<f32>,
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
    #[serde(default)]
    pub mcp: Vec<McpServerGrant>,
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
    Allowlist { allowlist: Vec<NetworkAllowEntry> },
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

/// One entry in `tools.mcp` (per ADR-018). The agent may connect to
/// `server_uri` and invoke any tool name in `allowed_tools`. Closed by
/// default — an MCP tool call against a server not listed here is denied.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpServerGrant {
    pub server_name: String,
    pub server_uri: String,
    pub allowed_tools: Vec<AllowedTool>,
}

/// One entry in [`McpServerGrant::allowed_tools`]. Two shapes are
/// accepted (per ADR-024 §"Decision" item 1):
///
/// 1. **String shorthand** — `"read_text_file"` — interpreted as
///    "no pre-validation; one-layer enforcement," preserving the
///    pre-ADR-024 behavior.
/// 2. **Object form** — `{ name, pre_validate }` — declares
///    side-effect clauses the mediator runs against
///    `tools.filesystem.*` / `tools.network.*` policy before
///    dispatching to the MCP server.
///
/// Both shapes deserialize via `#[serde(untagged)]` — serde tries
/// each variant in order and picks the first that matches. The
/// JSON Schema `oneOf` in
/// [`schemas/manifest/v1/manifest.schema.json`](../../../../schemas/manifest/v1/manifest.schema.json)
/// pins the cross-language contract.
///
/// Helper accessors keep call sites that only need the name terse:
///
/// ```ignore
/// for entry in &grant.allowed_tools {
///     if entry.name() == requested {
///         for clause in entry.pre_validate() { /* ... */ }
///     }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AllowedTool {
    /// Bare tool name — legacy/short-form. The mediator only enforces
    /// the MCP allowlist for this entry; the underlying tool call's
    /// side-effects are left to the MCP server's own discretion.
    Name(String),
    /// Object form with per-tool pre-validation clauses. The mediator
    /// runs each clause against the corresponding `tools.filesystem.*`
    /// / `tools.network.*` gate before dispatching to the MCP server.
    WithPreValidate {
        /// Tool name (matches the MCP server's tool catalog).
        name: String,
        /// Side-effect clauses the mediator pre-validates. Empty / absent
        /// `pre_validate` is equivalent to the [`AllowedTool::Name`]
        /// shorthand — one-layer MCP enforcement only.
        #[serde(default)]
        pre_validate: Vec<PreValidateClause>,
    },
}

impl AllowedTool {
    /// Tool name, regardless of which form the manifest uses.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            AllowedTool::Name(s) => s,
            AllowedTool::WithPreValidate { name, .. } => name,
        }
    }

    /// Pre-validation clauses declared for this tool. Empty for the
    /// string-shorthand form — i.e., one-layer MCP enforcement only.
    #[must_use]
    pub fn pre_validate(&self) -> &[PreValidateClause] {
        match self {
            AllowedTool::Name(_) => &[],
            AllowedTool::WithPreValidate { pre_validate, .. } => pre_validate,
        }
    }
}

/// One side-effect-shaped pre-validation clause for an [`AllowedTool`]
/// object form. The mediator extracts the named argument from the MCP
/// tool call's payload and runs the corresponding `policy.check_*`
/// method before dispatching the call (per ADR-024 §"Decision" item 2).
///
/// Phase 1 covers `filesystem_{read,write,delete}` + `network_outbound`;
/// `exec_run` is intentionally out of scope (exec via MCP is rare and
/// the path-extraction shape differs).
///
/// Exactly one of [`Self::arg`] / [`Self::arg_array`] must be set —
/// the JSON Schema enforces this via `oneOf` and the Rust deserializer
/// surfaces a typed error if both or neither are present.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PreValidateClause {
    /// Which side-effect family to gate against.
    pub kind: PreValidateKind,
    /// Name of the scalar argument carrying the path or URL the
    /// mediator should extract and check. Mutually exclusive with
    /// [`Self::arg_array`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arg: Option<String>,
    /// Name of an array-of-strings argument; the mediator extracts
    /// each element and runs the check on it. Mutually exclusive with
    /// [`Self::arg`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arg_array: Option<String>,
}

/// Side-effect family a [`PreValidateClause`] gates against. Adding a
/// new family requires a corresponding `policy.check_*` method, a JSON
/// Schema enum bump, and parity in the Go union type.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PreValidateKind {
    /// Path extracted from the named arg; checked via `policy.check_filesystem_read`.
    FilesystemRead,
    /// Path extracted from the named arg; checked via `policy.check_filesystem_write`.
    FilesystemWrite,
    /// Path extracted from the named arg; checked via `policy.check_filesystem_delete`.
    FilesystemDelete,
    /// Host + port parsed from the named arg (URL or host:port);
    /// checked via `policy.check_network_outbound`.
    NetworkOutbound,
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

/// One entry in `exec_grants`. `program` is either an absolute path or
/// a bare basename. `args_match` is parsed in Phase 1 and enforced when
/// the runtime can pass argv to the gate.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecGrant {
    pub program: String,
    #[serde(default, rename = "args_match")]
    pub args_match: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalClass {
    AnyWrite,
    AnyDelete,
    AnyNetworkOutbound,
    AnyExec,
}
