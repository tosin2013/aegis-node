use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("session_id mismatch: writer expected {expected:?}, got {got:?}")]
    SessionIdMismatch { expected: String, got: String },

    #[error("payload key {0:?} collides with a chain field")]
    PayloadKeyConflict(String),
}
