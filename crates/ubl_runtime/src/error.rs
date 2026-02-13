use thiserror::Error;

#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("validation: {0}")]
    Validation(String),
    #[error("binding: missing inputs: {missing:?}, available vars: {available:?}")]
    Binding { missing: Vec<String>, available: Vec<String> },
    #[error("policy deny: {0}")]
    PolicyDeny(String),
    #[error("engine: {0}")]
    Engine(String),
    #[error("serde-json: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, RuntimeError>;
