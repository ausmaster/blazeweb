use thiserror::Error;

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("parse error: {0}")]
    Parse(String),

    #[error("js error: {message}")]
    JsExecution {
        message: String,
        stack: Option<String>,
    },

    #[error("network error fetching {url}: {reason}")]
    Network { url: String, reason: String },

    #[error("timeout after {0:.1}s")]
    Timeout(f64),

    #[error("serialization error: {0}")]
    Serialize(String),
}
