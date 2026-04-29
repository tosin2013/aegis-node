use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

/// Errors the MCP client can produce. The mediator surfaces these as
/// `Error::Denied` (for policy refusals from the server side) or
/// generic transport errors at the runtime boundary.
#[derive(Debug, Error)]
pub enum Error {
    /// The server returned a JSON-RPC error envelope.
    #[error("mcp server error: code={code} message={message}")]
    ServerError { code: i64, message: String },

    /// The transport scheme of `server_uri` is not implemented in
    /// Phase 1. (Today we accept `stdio:` only; HTTP/SSE is a follow-up.)
    #[error("mcp: unsupported transport scheme in server_uri: {server_uri}")]
    UnsupportedTransport { server_uri: String },

    /// Spawning the child process failed (binary missing, permission
    /// denied, ...).
    #[error("mcp: spawn {server_uri:?} failed: {source}")]
    Spawn {
        server_uri: String,
        #[source]
        source: std::io::Error,
    },

    /// Generic transport / framing error.
    #[error("mcp protocol: {0}")]
    Protocol(String),
}
