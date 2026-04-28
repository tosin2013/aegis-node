use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("certificate generation: {0}")]
    Rcgen(#[from] rcgen::Error),

    #[error("certificate parse: {0}")]
    CertParse(String),

    #[error("invalid SPIFFE ID {input:?}: {reason}")]
    InvalidSpiffeId { input: String, reason: &'static str },

    #[error("invalid trust domain {0:?}")]
    InvalidTrustDomain(String),

    #[error("CA already initialized at {0}")]
    CaAlreadyInitialized(String),

    #[error("CA not initialized at {0}")]
    CaNotInitialized(String),

    #[error("digest must be 32 bytes (got {0})")]
    InvalidDigestLength(usize),
}
