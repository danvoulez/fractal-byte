use thiserror::Error;

#[derive(Error, Debug)]
pub enum AdapterError {
    #[error("adapter: {0}")]
    General(String),

    #[error("http: {0}")]
    Http(String),

    #[error("policy: adapter '{adapter}' not allowed by policy")]
    PolicyDeny { adapter: String },

    #[error("timeout: adapter '{adapter}' exceeded {timeout_ms}ms")]
    Timeout { adapter: String, timeout_ms: u64 },

    #[error("cid mismatch: expected {expected}, got {actual}")]
    CidMismatch { expected: String, actual: String },

    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, AdapterError>;
