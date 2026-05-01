use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("identity: {0}")]
    Identity(#[from] aegis_identity::Error),

    #[error("policy: {0}")]
    Policy(#[from] aegis_policy::Error),

    #[error("ledger: {0}")]
    Ledger(#[from] aegis_ledger_writer::Error),

    #[error("serialization: {0}")]
    Serde(#[from] serde_json::Error),

    #[error(
        "issued SVID does not bind the digests we passed to it (digest_field={field:?}); \
         likely an aegis-identity bug"
    )]
    SvidSelfCheck { field: String },

    /// The chat-template sidecar (`chat_template.sha256.txt` produced by
    /// `aegis pull`) is missing, unreadable, or doesn't carry a 64-char
    /// hex SHA-256. Per ADR-022 the F1 boot path refuses rather than
    /// silently omit the chat-template binding.
    #[error("chat-template sidecar at {path:?}: {detail}")]
    ChatTemplateSidecar { path: String, detail: String },

    /// `Session::run_turn` was called but no [`crate::backend::LoadedModel`]
    /// is attached. Use [`crate::Session::with_loaded_model`] after boot.
    #[error("no inference backend configured for session (call with_loaded_model first)")]
    NoBackendConfigured,

    /// The model returned an error from its `infer` call. The detail
    /// carries the impl-specific reason; the kind discriminates so the
    /// mediator can decide whether to halt or continue.
    #[error("backend infer failed: {0}")]
    BackendInfer(#[from] crate::backend::BackendError),

    /// `run_turn` parsed a tool call whose name doesn't match the
    /// expected `<server>__<tool>` shape (per ADR-018 LLM-B contract).
    /// The model emitted something the runtime can't dispatch — record
    /// the violation, don't try to interpret.
    #[error("tool call name {name:?} is not in the expected <server>__<tool> shape")]
    UnroutableToolCall { name: String },

    /// The manifest declared `tools.mcp[].server_name` shadows a name
    /// reserved for native dispatch (`filesystem`, `network`, `exec`).
    /// Rejected at boot rather than letting the collision surface
    /// silently when the model emits a tool call. Per [#92](https://github.com/tosin2013/aegis-node/issues/92).
    #[error(
        "manifest's tools.mcp[].server_name {name:?} collides with a reserved native \
         namespace; rename the MCP server (reserved: filesystem, network, exec)"
    )]
    ReservedMcpServerName { name: String },

    #[error("access-log: {0}")]
    AccessLog(#[from] aegis_access_log::Error),

    /// An ADR-024-A `pre_validate` clause names an arg that's missing
    /// from the tool call's payload, or is the wrong shape (e.g. clause
    /// declared `arg: path` but the payload has no `path` field, or
    /// the field is not a string). Surfaced as a denial — the model's
    /// tool call gets a Violation entry rather than crashing the
    /// session or silently dispatching.
    #[error("pre_validate clause for mcp tool {server:?}/{tool:?} arg {arg:?}: {reason}")]
    McpPreValidateMalformedArg {
        /// MCP server name from `tools.mcp[]`.
        server: String,
        /// Tool name from `allowed_tools`.
        tool: String,
        /// Arg name the clause referenced.
        arg: String,
        /// What's wrong (missing, not-a-string, not-an-array, etc.).
        reason: String,
    },

    #[error("denied: {reason}")]
    Denied { reason: String },

    #[error("requires approval: {reason}")]
    RequireApproval { reason: String },
}
