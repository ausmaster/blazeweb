//! Error types. All engine errors surface to Python as `RuntimeError` with a
//! descriptive string. No custom Python exception hierarchy in v2.0 — easy to
//! add later if users ask.

use pyo3::PyErr;
use pyo3::exceptions::PyRuntimeError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BlazeError {
    #[error(
        "chrome binary not found — pass chrome_path=, set BLAZEWEB_CHROME, or install chromium ({0})"
    )]
    ChromeNotFound(String),

    #[error("browser launch failed: {0}")]
    LaunchFailed(String),

    #[error("navigation to {url} did not reach lifecycle event {wait_until} within {timeout_ms}ms")]
    NavigationTimeout {
        timeout_ms: u64,
        url: String,
        wait_until: &'static str,
    },

    #[error("post_load_scripts[{index}]: {source}")]
    PostLoadScript {
        index: usize,
        #[source]
        source: Box<BlazeError>,
    },

    #[error("CDP: {0}")]
    Cdp(String),

    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    #[error("invalid config: {0}")]
    InvalidConfig(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("internal: {0}")]
    Internal(String),
}

impl BlazeError {
    pub fn cdp<E: std::fmt::Display>(e: E) -> Self {
        Self::Cdp(e.to_string())
    }
}

impl From<BlazeError> for PyErr {
    fn from(e: BlazeError) -> Self {
        PyRuntimeError::new_err(e.to_string())
    }
}

impl From<chromiumoxide::error::CdpError> for BlazeError {
    fn from(e: chromiumoxide::error::CdpError) -> Self {
        BlazeError::Cdp(e.to_string())
    }
}

impl From<url::ParseError> for BlazeError {
    fn from(e: url::ParseError) -> Self {
        BlazeError::InvalidUrl(e.to_string())
    }
}

/// Allow `?` to lift PyErr into BlazeError inside Rust-only code paths (e.g.
/// when parsing pydantic-dict config via PyO3 dict access).
impl From<PyErr> for BlazeError {
    fn from(e: PyErr) -> Self {
        BlazeError::InvalidConfig(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, BlazeError>;
