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

    #[error("access-log: {0}")]
    AccessLog(#[from] aegis_access_log::Error),

    #[error("denied: {reason}")]
    Denied { reason: String },

    #[error("requires approval: {reason}")]
    RequireApproval { reason: String },
}
