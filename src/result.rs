//! PyO3-visible output types returned from engine operations.
//!
//! `RawRenderOutput` and `RawFetchOutput` are simple data containers. The Python
//! `__init__.py` wraps them into `RenderResult` (str subclass) and `FetchResult`
//! (dataclass-ish). We keep the Rust side minimal and let Python shape the UX.

use pyo3::prelude::*;

use crate::dom::Dom;

/// One captured ``console.*`` event (or uncaught exception). The Python side
/// wraps these into the user-facing ``ConsoleMessage`` dataclass.
#[pyclass(name = "_ConsoleMessage", frozen)]
#[derive(Clone, Debug)]
pub struct ConsoleMessageRs {
    /// The console method that fired, lowercase: ``"log"`` / ``"info"`` /
    /// ``"warning"`` / ``"error"`` / ``"debug"`` / ``"trace"``. Uncaught
    /// exceptions appear as ``"error"``.
    #[pyo3(get, name = "type")]
    pub kind: String,
    /// The rendered message text (chrome stringifies non-string args before
    /// the event fires; this is the joined result of all args).
    #[pyo3(get)]
    pub text: String,
    /// ``time.time()`` (Unix epoch seconds, f64) at the moment the event was
    /// captured by the Rust listener.
    #[pyo3(get)]
    pub timestamp: f64,
}

/// Output of a single fetch (HTML-only). Constructed by engine; passed to Python.
#[pyclass(name = "_RenderOutput")]
#[derive(Clone)]
pub struct RawRenderOutput {
    #[pyo3(get)]
    pub html: String,
    #[pyo3(get)]
    pub console_messages: Vec<ConsoleMessageRs>,
    #[pyo3(get)]
    pub final_url: String,
    #[pyo3(get)]
    pub status_code: u16,
    #[pyo3(get)]
    pub elapsed_s: f64,
    /// One JSON-string entry per ``FetchConfig.post_load_scripts`` entry, in
    /// input order. ``None`` when the script returned ``undefined`` or a
    /// non-JSON-serializable value (DOM node, function). Python-side
    /// (`_make_render_result`) ``json.loads`` each entry into Python natives.
    #[pyo3(get)]
    pub post_load_results: Vec<Option<String>>,
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
    pub console_messages: Vec<ConsoleMessageRs>,
    #[pyo3(get)]
    pub final_url: String,
    #[pyo3(get)]
    pub status_code: u16,
    #[pyo3(get)]
    pub elapsed_s: f64,
    /// See ``RawRenderOutput.post_load_results``.
    #[pyo3(get)]
    pub post_load_results: Vec<Option<String>>,
}

#[pymethods]
impl RawFetchOutput {
    fn make_dom(&self) -> Dom {
        Dom::from_html(self.html.clone())
    }
}
