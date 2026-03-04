use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use pyo3::prelude::*;

pub mod css;
pub mod dom;
mod engine;
mod error;
mod js;
mod net;

/// Raw render output from Rust — converted to Python RenderResult in __init__.py.
#[pyclass(name = "_RenderOutput")]
#[derive(Clone)]
struct RawRenderOutput {
    #[pyo3(get)]
    html: String,
    #[pyo3(get)]
    errors: Vec<String>,
}

impl From<engine::RenderOutput> for RawRenderOutput {
    fn from(output: engine::RenderOutput) -> Self {
        Self {
            html: output.html,
            errors: output.errors,
        }
    }
}

/// Render HTML with JavaScript execution.
///
/// Takes HTML bytes, parses the DOM, executes any JavaScript,
/// and returns a RawRenderOutput with `.html` and `.errors`.
#[pyfunction]
#[pyo3(signature = (html, /, *, base_url=None))]
fn render(py: Python<'_>, html: &[u8], base_url: Option<&str>) -> PyResult<RawRenderOutput> {
    py.allow_threads(|| {
        engine::render(html, base_url)
            .map(RawRenderOutput::from)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    })
}

/// Fetch a URL, render the HTML with JavaScript execution, and return the result.
#[pyfunction]
#[pyo3(signature = (url, /))]
fn fetch(py: Python<'_>, url: &str) -> PyResult<RawRenderOutput> {
    py.allow_threads(|| {
        engine::fetch(url)
            .map(RawRenderOutput::from)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    })
}

/// HTTP client with per-instance cache and cookie jar.
///
/// Each Client instance shares the global connection pool but has
/// independent HTTP cache and cookie jar. Cache behavior is controllable
/// at the class level and per-render/per-fetch call.
#[pyclass]
struct Client {
    cache: AtomicBool,
    cache_read: AtomicBool,
    cache_write: AtomicBool,
    http_cache: Arc<Mutex<net::http_cache::HttpCache>>,
    cookie_jar: Arc<Mutex<net::cookies::CookieJar>>,
}

impl Client {
    /// Build a `FetchContext` from this client's persistent state, with optional per-call overrides.
    fn build_context(
        &self,
        base_url: Option<&str>,
        cache: Option<bool>,
        cache_read: Option<bool>,
        cache_write: Option<bool>,
    ) -> net::fetch::FetchContext {
        // Per-call `cache=False` disables both read and write
        let master = cache.unwrap_or_else(|| self.cache.load(Ordering::Relaxed));
        let read = if master {
            cache_read.unwrap_or_else(|| self.cache_read.load(Ordering::Relaxed))
        } else {
            false
        };
        let write = if master {
            cache_write.unwrap_or_else(|| self.cache_write.load(Ordering::Relaxed))
        } else {
            false
        };

        net::fetch::FetchContext::with_shared(
            base_url,
            Arc::clone(&self.cookie_jar),
            Arc::clone(&self.http_cache),
            read,
            write,
        )
    }
}

#[pymethods]
impl Client {
    #[new]
    #[pyo3(signature = (*, cache=true, cache_read=true, cache_write=true))]
    fn new(cache: bool, cache_read: bool, cache_write: bool) -> Self {
        Self {
            cache: AtomicBool::new(cache),
            cache_read: AtomicBool::new(cache_read),
            cache_write: AtomicBool::new(cache_write),
            http_cache: Arc::new(Mutex::new(net::http_cache::HttpCache::new())),
            cookie_jar: Arc::new(Mutex::new(net::cookies::CookieJar::new())),
        }
    }

    /// Render HTML with JavaScript execution, using the persistent cache.
    #[pyo3(signature = (html, /, *, base_url=None, cache=None, cache_read=None, cache_write=None))]
    fn render(
        &self,
        py: Python<'_>,
        html: &[u8],
        base_url: Option<&str>,
        cache: Option<bool>,
        cache_read: Option<bool>,
        cache_write: Option<bool>,
    ) -> PyResult<RawRenderOutput> {
        let context = self.build_context(base_url, cache, cache_read, cache_write);
        py.allow_threads(|| {
            engine::render_with_context(html, base_url, &context)
                .map(RawRenderOutput::from)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }

    /// Fetch a URL and render it with JavaScript execution, using the persistent cache.
    #[pyo3(signature = (url, /, *, cache=None, cache_read=None, cache_write=None))]
    fn fetch(
        &self,
        py: Python<'_>,
        url: &str,
        cache: Option<bool>,
        cache_read: Option<bool>,
        cache_write: Option<bool>,
    ) -> PyResult<RawRenderOutput> {
        let context = self.build_context(Some(url), cache, cache_read, cache_write);
        py.allow_threads(|| {
            engine::fetch_with_context(url, &context)
                .map(RawRenderOutput::from)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }

    /// Flush all cached HTTP responses and cookies.
    fn clear_cache(&self) {
        if let Ok(mut cache) = self.http_cache.lock() {
            cache.clear();
        }
        if let Ok(mut jar) = self.cookie_jar.lock() {
            jar.clear();
        }
    }

    /// Number of HTTP responses currently cached.
    #[getter]
    fn cache_size(&self) -> usize {
        self.http_cache
            .lock()
            .map(|c| c.len())
            .unwrap_or(0)
    }

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
#[pyo3(name = "_blazeweb")]
fn blazeweb_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Initialize logging on first import. Controlled via RUST_LOG env var:
    //   RUST_LOG=blazeweb=info   — execution milestones
    //   RUST_LOG=blazeweb=debug  — per-script details
    //   RUST_LOG=blazeweb=trace  — verbose (timer/fetch drain)
    let _ = env_logger::try_init();
    m.add_function(wrap_pyfunction!(render, m)?)?;
    m.add_function(wrap_pyfunction!(fetch, m)?)?;
    m.add_class::<Client>()?;
    m.add_class::<RawRenderOutput>()?;
    Ok(())
}
