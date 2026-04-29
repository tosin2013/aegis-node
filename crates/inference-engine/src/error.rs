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
}
