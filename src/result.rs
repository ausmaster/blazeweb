//! PyO3-visible output types returned from engine operations.
//!
//! `RawRenderOutput` and `RawFetchOutput` are simple data containers. The Python
//! `__init__.py` wraps them into `RenderResult` (str subclass) and `FetchResult`
//! (dataclass-ish). We keep the Rust side minimal and let Python shape the UX.

use pyo3::prelude::*;

use crate::dom::Dom;

/// Output of a single fetch (HTML-only). Constructed by engine; passed to Python.
#[pyclass(name = "_RenderOutput")]
#[derive(Clone)]
pub struct RawRenderOutput {
    #[pyo3(get)]
    pub html: String,
    #[pyo3(get)]
    pub errors: Vec<String>,
    #[pyo3(get)]
    pub final_url: String,
    #[pyo3(get)]
    pub status_code: u16,
    #[pyo3(get)]
    pub elapsed_s: f64,
}

#[pymethods]
impl RawRenderOutput {
    /// Build a Dom from the HTML (lazy-parse on first query). Called from
    /// Python-side RenderResult.dom property.
    fn make_dom(&self) -> Dom {
        Dom::from_html(self.html.clone())
    }
}

/// Output of fetch_all — HTML + PNG from one page visit.
#[pyclass(name = "_FetchOutput")]
#[derive(Clone)]
pub struct RawFetchOutput {
    #[pyo3(get)]
    pub html: String,
    #[pyo3(get)]
    pub png: Vec<u8>,
    #[pyo3(get)]
    pub errors: Vec<String>,
    #[pyo3(get)]
    pub final_url: String,
    #[pyo3(get)]
    pub status_code: u16,
    #[pyo3(get)]
    pub elapsed_s: f64,
}

#[pymethods]
impl RawFetchOutput {
    fn make_dom(&self) -> Dom {
        Dom::from_html(self.html.clone())
    }
}
