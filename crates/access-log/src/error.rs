use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("ledger writer: {0}")]
    Ledger(#[from] aegis_ledger_writer::Error),

    #[error("serialization: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("resource_uri must be non-empty")]
    EmptyResourceUri,

    #[error("reasoning step input must be non-empty")]
    EmptyReasoningInput,
}
