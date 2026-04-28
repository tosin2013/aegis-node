use aegis_identity::DigestMismatch;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("ledger: {0}")]
    Ledger(#[from] aegis_ledger_writer::Error),

    #[error("identity: {0}")]
    Identity(#[from] aegis_identity::Error),

    #[error("serialization: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("manifest does not support extends: in Phase 1a (got {0} parents)")]
    ExtendsUnsupported(usize),

    #[error("identity digest binding violated: {0}")]
    IdentityRebind(DigestMismatch),
}
