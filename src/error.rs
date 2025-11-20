use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProxyError {
    #[error("Invalid Claude request: {0}")]
    InvalidClaudeRequest(String),

    #[error("Invalid Gemini response: {0}")]
    InvalidGeminiResponse(String),

    #[error("Transformation error: {0}")]
    TransformationError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("Upstream error: {0}")]
    UpstreamError(String),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Environment variable error: {0}")]
    EnvVarError(#[from] std::env::VarError),
}

pub type Result<T> = std::result::Result<T, ProxyError>;
