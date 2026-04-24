//! `Client` pyclass — the main entry point. Owns a chromiumoxide Browser and a
//! Semaphore, dispatches Python calls to the shared tokio runtime.
//!
//! All methods release the GIL via `py.allow_threads()` before entering Rust
//! work. N Python threads calling `Client.fetch()` concurrently all do real
//! parallel work inside tokio, capped by the configured concurrency semaphore.

use std::sync::Arc;

use chromiumoxide::{Browser, BrowserConfig};
use futures::StreamExt;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};
use tokio::sync::Semaphore;

use crate::chrome;
use crate::config::{
    parse_client_config, parse_fetch_config, parse_screenshot_config, ClientConfigRs,
    FetchConfigRs, ScreenshotConfigRs,
};
use crate::engine::{capture_page, CaptureMode};
use crate::error::{BlazeError, Result};
use crate::result::{RawFetchOutput, RawRenderOutput};
use crate::runtime;

/// Opaque wrapper for the chromium Browser + its handler task + limits.
///
/// `config` is wrapped in RwLock so runtime updates (see `Client::update_config`)
/// can swap it atomically without blocking in-flight fetches for long —
/// readers clone out at fetch start; the writer briefly takes exclusive access.
struct ClientState {
    runtime: Arc<tokio::runtime::Runtime>,
    browser: Arc<Browser>,
    handler_task: parking_lot::Mutex<Option<tokio::task::JoinHandle<()>>>,
    semaphore: Arc<Semaphore>,
    config: parking_lot::RwLock<ClientConfigRs>,
    closed: std::sync::atomic::AtomicBool,
}

impl ClientState {
    fn is_closed(&self) -> bool {
        self.closed.load(std::sync::atomic::Ordering::Acquire)
    }
}

#[pyclass]
pub struct Client {
    inner: Arc<ClientState>,
}

impl Client {
    fn check_open(&self) -> Result<()> {
        if self.inner.is_closed() {
            Err(BlazeError::Internal("Client is closed".to_string()))
        } else {
            Ok(())
        }
    }
}

#[pymethods]
impl Client {
    /// Constructor. Takes a dict form of `ClientConfig` (pydantic `.model_dump()`).
    /// Any field can be None to fall through to defaults.
    #[new]
    fn new(py: Python<'_>, config: &Bound<'_, PyAny>) -> PyResult<Self> {
        let config_rs = parse_client_config(config).map_err(PyErr::from)?;
        let chrome_path = chrome::resolve(config_rs.chrome.path.as_deref())
            .map_err(PyErr::from)?;

        let runtime = runtime::shared();

        // Build Chrome CLI args from config.
        let mut builder = BrowserConfig::builder()
            .chrome_executable(chrome_path)
            .arg("--headless=new")
            .arg("--disable-gpu")
            .arg("--no-sandbox")
            .arg("--hide-scrollbars")
            .arg("--disable-dev-shm-usage")
            .arg(format!(
                "--window-size={},{}",
                config_rs.viewport.width, config_rs.viewport.height
            ));

        if config_rs.network.ignore_https_errors {
            builder = builder.arg("--ignore-certificate-errors");
        }
        if let Some(proxy) = &config_rs.network.proxy {
            builder = builder.arg(format!("--proxy-server={proxy}"));
        }
        if let Some(user_data_dir) = &config_rs.chrome.user_data_dir {
            builder = builder.arg(format!("--user-data-dir={user_data_dir}"));
        }
        for arg in &config_rs.chrome.args {
            builder = builder.arg(arg.clone());
        }

        let cfg = builder
            .build()
            .map_err(|e| BlazeError::LaunchFailed(e.to_string()))?;

        let (browser, handler_task) = py.allow_threads(|| {
            runtime.block_on(async {
                let (browser, mut handler) = Browser::launch(cfg)
                    .await
                    .map_err(BlazeError::from)?;
                let task = tokio::spawn(async move {
                    while let Some(res) = handler.next().await {
                        if let Err(_) = res {
                            // Handler ended — browser will report errors on the next page op.
                            break;
                        }
                    }
                });
                Ok::<_, BlazeError>((browser, task))
            })
        })
        .map_err(PyErr::from)?;

        let state = ClientState {
            runtime: runtime.clone(),
            browser: Arc::new(browser),
            handler_task: parking_lot::Mutex::new(Some(handler_task)),
            semaphore: Arc::new(Semaphore::new(config_rs.concurrency.max(1))),
            config: parking_lot::RwLock::new(config_rs),
            closed: std::sync::atomic::AtomicBool::new(false),
        };

        Ok(Self { inner: Arc::new(state) })
    }

    /// Swap in a new config. Launch-only fields (chrome.*, concurrency, proxy,
    /// ignore_https_errors, launch_ms) are validated Python-side before this
    /// call — we just replace atomically. Next fetch sees the new values.
    fn update_config(&self, config: &Bound<'_, PyAny>) -> PyResult<()> {
        self.check_open().map_err(PyErr::from)?;
        let new_cfg = parse_client_config(config).map_err(PyErr::from)?;
        *self.inner.config.write() = new_cfg;
        Ok(())
    }

    /// Fetch URL → RawRenderOutput (HTML only).
    fn fetch(
        &self,
        py: Python<'_>,
        url: String,
        per_call: &Bound<'_, PyAny>,
    ) -> PyResult<RawRenderOutput> {
        self.check_open().map_err(PyErr::from)?;
        let fetch_cfg = parse_fetch_config(per_call).map_err(PyErr::from)?;
        let shot_cfg = ScreenshotConfigRs::default();
        let state = self.inner.clone();
        let runtime = state.runtime.clone();

        py.allow_threads(move || {
            runtime.block_on(async move {
                let _permit = state.semaphore.acquire().await.map_err(|e| {
                    BlazeError::Internal(format!("semaphore closed: {e}"))
                })?;
                let base_cfg = state.config.read().clone();
                let out = capture_page(
                    &state.browser,
                    &url,
                    &base_cfg,
                    &fetch_cfg,
                    &shot_cfg,
                    CaptureMode::Html,
                )
                .await?;
                Ok::<_, BlazeError>(RawRenderOutput {
                    html: out.html.unwrap_or_default(),
                    errors: out.errors,
                    final_url: out.final_url,
                    status_code: out.status_code,
                    elapsed_s: out.elapsed_s,
                })
            })
        })
        .map_err(PyErr::from)
    }

    /// Screenshot URL → PNG bytes.
    fn screenshot<'py>(
        &self,
        py: Python<'py>,
        url: String,
        per_shot: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        self.check_open().map_err(PyErr::from)?;
        let shot_cfg = parse_screenshot_config(per_shot).map_err(PyErr::from)?;
        let fetch_cfg = FetchConfigRs::default();
        let state = self.inner.clone();

        let runtime = state.runtime.clone();
        let png = py
            .allow_threads(move || {
                runtime.block_on(async move {
                    let _permit = state.semaphore.acquire().await.map_err(|e| {
                        BlazeError::Internal(format!("semaphore closed: {e}"))
                    })?;
                    let base_cfg = state.config.read().clone();
                    let out = capture_page(
                        &state.browser,
                        &url,
                        &base_cfg,
                        &fetch_cfg,
                        &shot_cfg,
                        CaptureMode::Png,
                    )
                    .await?;
                    Ok::<_, BlazeError>(out.png.unwrap_or_default())
                })
            })
            .map_err(PyErr::from)?;

        Ok(PyBytes::new(py, &png))
    }

    /// Fetch URL → RawFetchOutput (HTML + PNG from one visit).
    fn fetch_all(
        &self,
        py: Python<'_>,
        url: String,
        per_call: &Bound<'_, PyAny>,
        per_shot: &Bound<'_, PyAny>,
    ) -> PyResult<RawFetchOutput> {
        self.check_open().map_err(PyErr::from)?;
        let fetch_cfg = parse_fetch_config(per_call).map_err(PyErr::from)?;
        let shot_cfg = parse_screenshot_config(per_shot).map_err(PyErr::from)?;
        let state = self.inner.clone();
        let runtime = state.runtime.clone();

        py.allow_threads(move || {
            runtime.block_on(async move {
                let _permit = state.semaphore.acquire().await.map_err(|e| {
                    BlazeError::Internal(format!("semaphore closed: {e}"))
                })?;
                let base_cfg = state.config.read().clone();
                let out = capture_page(
                    &state.browser,
                    &url,
                    &base_cfg,
                    &fetch_cfg,
                    &shot_cfg,
                    CaptureMode::Both,
                )
                .await?;
                Ok::<_, BlazeError>(RawFetchOutput {
                    html: out.html.unwrap_or_default(),
                    png: out.png.unwrap_or_default(),
                    errors: out.errors,
                    final_url: out.final_url,
                    status_code: out.status_code,
                    elapsed_s: out.elapsed_s,
                })
            })
        })
        .map_err(PyErr::from)
    }

    /// Batch of URLs (parallel inside Rust tokio). Returns list of results
    /// matching the `capture` mode: "html" → list[RawRenderOutput],
    /// "png" → list[bytes], "both" → list[RawFetchOutput].
    fn batch<'py>(
        &self,
        py: Python<'py>,
        urls: Vec<String>,
        capture: &str,
        per_call: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyList>> {
        self.check_open().map_err(PyErr::from)?;
        let mode = match capture {
            "html" => CaptureMode::Html,
            "png" => CaptureMode::Png,
            "both" => CaptureMode::Both,
            other => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "capture must be 'html'|'png'|'both', got {other:?}"
                )))
            }
        };
        let fetch_cfg = parse_fetch_config(per_call).map_err(PyErr::from)?;
        let state = self.inner.clone();
        let runtime = state.runtime.clone();

        #[allow(clippy::type_complexity)]
        let outputs: Vec<std::result::Result<crate::engine::CaptureOutput, BlazeError>> =
            py.allow_threads(move || {
                runtime.block_on(async move {
                    let shot_cfg = ScreenshotConfigRs::default();
                    // Snapshot config ONCE for the whole batch — in-batch updates don't re-apply.
                    let base_cfg = state.config.read().clone();
                    let tasks: Vec<_> = urls
                        .iter()
                        .cloned()
                        .map(|url| {
                            let browser = state.browser.clone();
                            let sem = state.semaphore.clone();
                            let base = base_cfg.clone();
                            let fc = fetch_cfg.clone();
                            let sc = shot_cfg.clone();
                            tokio::spawn(async move {
                                let _permit = sem.acquire().await.map_err(|e| {
                                    BlazeError::Internal(format!("sem: {e}"))
                                })?;
                                capture_page(&browser, &url, &base, &fc, &sc, mode).await
                            })
                        })
                        .collect();
                    let mut collected = Vec::with_capacity(tasks.len());
                    for h in tasks {
                        let r = match h.await {
                            Ok(inner) => inner,
                            Err(e) => Err(BlazeError::Internal(format!("join: {e}"))),
                        };
                        collected.push(r);
                    }
                    collected
                })
            });

        let results = PyList::empty(py);
        for r in outputs {
            match r {
                Ok(out) => match mode {
                    CaptureMode::Html => {
                        let raw = RawRenderOutput {
                            html: out.html.unwrap_or_default(),
                            errors: out.errors,
                            final_url: out.final_url,
                            status_code: out.status_code,
                            elapsed_s: out.elapsed_s,
                        };
                        results.append(raw)?;
                    }
                    CaptureMode::Png => {
                        results.append(PyBytes::new(py, &out.png.unwrap_or_default()))?;
                    }
                    CaptureMode::Both => {
                        let raw = RawFetchOutput {
                            html: out.html.unwrap_or_default(),
                            png: out.png.unwrap_or_default(),
                            errors: out.errors,
                            final_url: out.final_url,
                            status_code: out.status_code,
                            elapsed_s: out.elapsed_s,
                        };
                        results.append(raw)?;
                    }
                },
                Err(e) => {
                    // For v1.0, raise on first failure. Future: accept a partial-results flag.
                    return Err(PyErr::from(e));
                }
            }
        }
        Ok(results)
    }

    /// Explicit shutdown. Drops the Browser (chromium quits) and joins the handler task.
    fn close(&self, py: Python<'_>) -> PyResult<()> {
        if self.inner.is_closed() {
            return Ok(());
        }
        self.inner
            .closed
            .store(true, std::sync::atomic::Ordering::Release);
        let state = self.inner.clone();
        let runtime = state.runtime.clone();
        py.allow_threads(move || {
            runtime.block_on(async move {
                // Drop our strong Browser ref; any in-flight tasks will notice closure.
                // The handler task will end when the Browser's receiver stream closes.
                if let Some(task) = state.handler_task.lock().take() {
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), task).await;
                }
            });
        });
        Ok(())
    }

    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    #[pyo3(signature = (_exc_type=None, _exc_val=None, _exc_tb=None))]
    fn __exit__(
        &self,
        py: Python<'_>,
        _exc_type: Option<PyObject>,
        _exc_val: Option<PyObject>,
        _exc_tb: Option<PyObject>,
    ) -> PyResult<()> {
        self.close(py)
    }
}
