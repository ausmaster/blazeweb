use pyo3::prelude::*;

pub mod dom;
mod engine;
mod error;
mod js;
mod net;

/// Render HTML with JavaScript execution.
///
/// Takes HTML bytes, parses the DOM, executes any JavaScript,
/// and returns the fully resolved HTML as a string.
#[pyfunction]
#[pyo3(signature = (html, /, *, base_url=None))]
fn render(py: Python<'_>, html: &[u8], base_url: Option<&str>) -> PyResult<String> {
    py.allow_threads(|| {
        engine::render(html, base_url).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(e.to_string())
        })
    })
}

#[pymodule]
#[pyo3(name = "_blazeclient")]
fn blazeclient_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(render, m)?)?;
    Ok(())
}
