use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

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

/// HTTP client with per-instance script cache for external `<script src>` fetches.
///
/// Each Client maintains its own cache — separate clients have separate caches.
/// Cache behavior is controllable at both the class level and per-render call.
#[pyclass]
struct Client {
    script_cache: net::fetch::ScriptCache,
    cache: AtomicBool,
    cache_read: AtomicBool,
    cache_write: AtomicBool,
}

#[pymethods]
impl Client {
    #[new]
    #[pyo3(signature = (*, cache=true, cache_read=true, cache_write=true))]
    fn new(cache: bool, cache_read: bool, cache_write: bool) -> Self {
        Self {
            script_cache: Mutex::new(HashMap::new()),
            cache: AtomicBool::new(cache),
            cache_read: AtomicBool::new(cache_read),
            cache_write: AtomicBool::new(cache_write),
        }
    }

    /// Render HTML with JavaScript execution, using the script cache.
    ///
    /// Per-render kwargs override class-level settings.
    /// `cache=False` disables both read and write for this call.
    #[pyo3(signature = (html, /, *, base_url=None, cache=None, cache_read=None, cache_write=None))]
    fn render(
        &self,
        py: Python<'_>,
        html: &[u8],
        base_url: Option<&str>,
        cache: Option<bool>,
        cache_read: Option<bool>,
        cache_write: Option<bool>,
    ) -> PyResult<String> {
        // Resolve: per-render kwarg > class-level setting
        let master = cache.unwrap_or_else(|| self.cache.load(Ordering::Relaxed));
        let do_read = master && cache_read.unwrap_or_else(|| self.cache_read.load(Ordering::Relaxed));
        let do_write = master && cache_write.unwrap_or_else(|| self.cache_write.load(Ordering::Relaxed));

        if do_read || do_write {
            let opts = net::fetch::CacheOpts {
                cache: &self.script_cache,
                read: do_read,
                write: do_write,
            };
            py.allow_threads(|| {
                engine::render_with_cache(html, base_url, Some(&opts)).map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(e.to_string())
                })
            })
        } else {
            py.allow_threads(|| {
                engine::render(html, base_url).map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(e.to_string())
                })
            })
        }
    }

    /// Flush all cached scripts.
    fn clear_cache(&self) {
        self.script_cache.lock().unwrap().clear();
    }

    /// Number of scripts currently cached.
    #[getter]
    fn cache_size(&self) -> usize {
        self.script_cache.lock().unwrap().len()
    }

    // --- cache flag getters/setters (use &self via AtomicBool) ---

    #[getter]
    fn get_cache(&self) -> bool {
        self.cache.load(Ordering::Relaxed)
    }

    #[setter]
    fn set_cache(&self, value: bool) {
        self.cache.store(value, Ordering::Relaxed);
    }

    #[getter]
    fn get_cache_read(&self) -> bool {
        self.cache_read.load(Ordering::Relaxed)
    }

    #[setter]
    fn set_cache_read(&self, value: bool) {
        self.cache_read.store(value, Ordering::Relaxed);
    }

    #[getter]
    fn get_cache_write(&self) -> bool {
        self.cache_write.load(Ordering::Relaxed)
    }

    #[setter]
    fn set_cache_write(&self, value: bool) {
        self.cache_write.store(value, Ordering::Relaxed);
    }
}

#[pymodule]
#[pyo3(name = "_blazeclient")]
fn blazeclient_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(render, m)?)?;
    m.add_class::<Client>()?;
    Ok(())
}
