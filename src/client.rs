//! `Client` pyclass — the main entry point. Owns a chromiumoxide Browser and a
//! Semaphore, dispatches Python calls to the shared tokio runtime.
//!
//! Two callable shapes, one implementation:
//! - **Sync methods** (`fetch`, `screenshot`, `fetch_all`, `batch`, `close`)
//!   release the GIL via `py.allow_threads()` and `block_on()` the work on
//!   the shared runtime. N Python threads can call them concurrently and
//!   make real parallel progress; the page-pool semaphore caps in-flight
//!   pages at `concurrency`.
//! - **Async methods** (`fetch_async`, `screenshot_async`, `fetch_all_async`,
//!   `batch_async`, `close_async`) bridge to Python via
//!   `pyo3_async_runtimes::tokio::future_into_py`. They return Python
//!   awaitables that callers `await` from an asyncio event loop. No
//!   `allow_threads` needed — the bridge handles GIL release.
//!
//! Both forms route through the same `do_*_inner` async helpers, so there's
//! exactly one implementation of each operation and two callable shapes.

use std::sync::Arc;

use chromiumoxide::{Browser, BrowserConfig};
use futures::StreamExt;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};

use crate::chrome;
use crate::config::{
    ClientConfigRs, FetchConfigRs, ScreenshotConfigRs, parse_client_config, parse_fetch_config,
    parse_screenshot_config,
};
use crate::engine::{CaptureMode, CaptureOutput, capture_page};
use crate::error::{BlazeError, Result};
use crate::pool::PagePool;
use crate::result::{ConsoleMessageRs, RawFetchOutput, RawRenderOutput};
use crate::runtime;

/// Chromium Browser + page pool + handler task. `config` is RwLock-wrapped so
/// `update_config` can swap atomically without blocking in-flight fetches.
struct ClientState {
    runtime: Arc<tokio::runtime::Runtime>,
    /// Keeps the browser process alive while the pool exists.
    #[allow(dead_code)]
    browser: Arc<Browser>,
    pool: Arc<PagePool>,
    handler_task: parking_lot::Mutex<Option<tokio::task::JoinHandle<()>>>,
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

// ---------------------------------------------------------------------------
// Async helpers — shared by sync (`block_on`) and async (`future_into_py`)
// entry points. Free functions so each can `state.clone()` ownership without
// borrowing `&self` across an await point.
// ---------------------------------------------------------------------------

async fn do_fetch_inner(
    state: Arc<ClientState>,
    url: String,
    fetch_cfg: FetchConfigRs,
) -> Result<RawRenderOutput> {
    let shot_cfg = ScreenshotConfigRs::default();
    let guard = state.pool.acquire().await?;
    let base_cfg = state.config.read().clone();
    let out = capture_page(
        &guard,
        &url,
        &base_cfg,
        &fetch_cfg,
        &shot_cfg,
        CaptureMode::Html,
    )
    .await?;
    Ok(RawRenderOutput {
        html: out.html.unwrap_or_default(),
        console_messages: out.console_messages,
        final_url: out.final_url,
        status_code: out.status_code,
        elapsed_s: out.elapsed_s,
        post_load_results: out.post_load_results,
    })
}

async fn do_screenshot_inner(
    state: Arc<ClientState>,
    url: String,
    shot_cfg: ScreenshotConfigRs,
) -> Result<Vec<u8>> {
    let fetch_cfg = FetchConfigRs::default();
    let guard = state.pool.acquire().await?;
    let base_cfg = state.config.read().clone();
    let out = capture_page(
        &guard,
        &url,
        &base_cfg,
        &fetch_cfg,
        &shot_cfg,
        CaptureMode::Png,
    )
    .await?;
    Ok(out.png.unwrap_or_default())
}

async fn do_fetch_all_inner(
    state: Arc<ClientState>,
    url: String,
    fetch_cfg: FetchConfigRs,
    shot_cfg: ScreenshotConfigRs,
) -> Result<RawFetchOutput> {
    let guard = state.pool.acquire().await?;
    let base_cfg = state.config.read().clone();
    let out = capture_page(
        &guard,
        &url,
        &base_cfg,
        &fetch_cfg,
        &shot_cfg,
        CaptureMode::Both,
    )
    .await?;
    Ok(RawFetchOutput {
        html: out.html.unwrap_or_default(),
        png: out.png.unwrap_or_default(),
        console_messages: out.console_messages,
        final_url: out.final_url,
        status_code: out.status_code,
        elapsed_s: out.elapsed_s,
        post_load_results: out.post_load_results,
    })
}

/// Run a batch of URLs in parallel. Returns one `Result` per URL — partial
/// failures are surfaced per-item rather than aborting the batch.
async fn do_batch_inner(
    state: Arc<ClientState>,
    urls: Vec<String>,
    fetch_cfg: FetchConfigRs,
    mode: CaptureMode,
) -> Vec<std::result::Result<CaptureOutput, BlazeError>> {
    let shot_cfg = ScreenshotConfigRs::default();
    // Snapshot config ONCE for the whole batch — in-batch updates don't re-apply.
    let base_cfg = state.config.read().clone();
    let tasks: Vec<_> = urls
        .into_iter()
        .map(|url| {
            let pool = state.pool.clone();
            let base = base_cfg.clone();
            let fc = fetch_cfg.clone();
            let sc = shot_cfg.clone();
            tokio::spawn(async move {
                let guard = pool.acquire().await?;
                capture_page(&guard, &url, &base, &fc, &sc, mode).await
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
}

async fn do_close_inner(state: Arc<ClientState>) {
    state.pool.close_all().await;
    // Drop the MutexGuard before any await — `take()` detaches the
    // JoinHandle so we can join it without holding the lock.
    let task_opt = state.handler_task.lock().take();
    if let Some(task) = task_opt {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), task).await;
    }
}

/// Append one batch result to `list`, with per-item failure → stub-result
/// fallback. Shared by sync and async batch wrappers.
fn batch_result_to_py(
    py: Python<'_>,
    result: std::result::Result<CaptureOutput, BlazeError>,
    mode: CaptureMode,
    list: &Bound<'_, PyList>,
) -> PyResult<()> {
    match result {
        Ok(out) => match mode {
            CaptureMode::Html => list.append(RawRenderOutput {
                html: out.html.unwrap_or_default(),
                console_messages: out.console_messages,
                final_url: out.final_url,
                status_code: out.status_code,
                elapsed_s: out.elapsed_s,
                post_load_results: out.post_load_results,
            }),
            CaptureMode::Png => list.append(PyBytes::new(py, &out.png.unwrap_or_default())),
            CaptureMode::Both => list.append(RawFetchOutput {
                html: out.html.unwrap_or_default(),
                png: out.png.unwrap_or_default(),
                console_messages: out.console_messages,
                final_url: out.final_url,
                status_code: out.status_code,
                elapsed_s: out.elapsed_s,
                post_load_results: out.post_load_results,
            }),
        },
        Err(e) => {
            log::warn!("batch item failed: {e}");
            // Synthesize a single error-level ConsoleMessage so the stub
            // result still surfaces the failure via `RenderResult.errors`.
            // Timestamp is 0.0 — there's no real event time for an internal
            // failure.
            let stub_err = vec![ConsoleMessageRs {
                kind: "error".to_string(),
                text: e.to_string(),
                timestamp: 0.0,
            }];
            match mode {
                CaptureMode::Html => list.append(RawRenderOutput {
                    html: String::new(),
                    console_messages: stub_err,
                    final_url: String::new(),
                    status_code: 0,
                    elapsed_s: 0.0,
                    post_load_results: Vec::new(),
                }),
                CaptureMode::Png => list.append(PyBytes::new(py, b"")),
                CaptureMode::Both => list.append(RawFetchOutput {
                    html: String::new(),
                    png: Vec::new(),
                    console_messages: stub_err,
                    final_url: String::new(),
                    status_code: 0,
                    elapsed_s: 0.0,
                    post_load_results: Vec::new(),
                }),
            }
        }
    }
}

fn parse_capture_mode(capture: &str) -> PyResult<CaptureMode> {
    match capture {
        "html" => Ok(CaptureMode::Html),
        "png" => Ok(CaptureMode::Png),
        "both" => Ok(CaptureMode::Both),
        other => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "capture must be 'html'|'png'|'both', got {other:?}"
        ))),
    }
}

#[pymethods]
impl Client {
    /// Constructor. Takes a dict form of `ClientConfig` (pydantic `.model_dump()`).
    /// Any field can be None to fall through to defaults.
    #[new]
    fn new(py: Python<'_>, config: &Bound<'_, PyAny>) -> PyResult<Self> {
        let config_rs = parse_client_config(config).map_err(PyErr::from)?;
        let chrome_path = chrome::resolve(config_rs.chrome.path.as_deref()).map_err(PyErr::from)?;
        let chrome_display = chrome_path.display().to_string();

        let runtime = runtime::shared();

        // Chrome CLI: the curated Puppeteer/Playwright-style headless speedup
        // flags strip background services, translation, extensions, sync, etc.
        let mut builder = BrowserConfig::builder()
            .chrome_executable(chrome_path)
            .arg("--headless=new")
            .arg("--disable-gpu")
            .arg("--no-sandbox")
            .arg("--hide-scrollbars")
            .arg("--disable-dev-shm-usage")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--disable-background-networking")
            .arg("--disable-background-timer-throttling")
            .arg("--disable-backgrounding-occluded-windows")
            .arg("--disable-breakpad")
            .arg("--disable-client-side-phishing-detection")
            .arg("--disable-component-extensions-with-background-pages")
            .arg("--disable-component-update")
            .arg("--disable-default-apps")
            .arg("--disable-domain-reliability")
            .arg("--disable-extensions")
            .arg("--disable-features=Translate,BackForwardCache,AcceptCHFrame,MediaRouter,OptimizationHints,IsolateOrigins,site-per-process")
            .arg("--disable-hang-monitor")
            .arg("--disable-ipc-flooding-protection")
            .arg("--disable-popup-blocking")
            .arg("--disable-prompt-on-repost")
            .arg("--disable-renderer-backgrounding")
            .arg("--disable-sync")
            .arg("--metrics-recording-only")
            .arg("--mute-audio")
            .arg("--password-store=basic")
            .arg("--use-mock-keychain")
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

        log::info!(
            target: "blazeweb::client",
            "launching chrome ({} concurrency, viewport {}x{}, chrome={})",
            config_rs.concurrency,
            config_rs.viewport.width,
            config_rs.viewport.height,
            chrome_display
        );

        let cfg = builder
            .build()
            .map_err(|e| BlazeError::LaunchFailed(e.to_string()))?;

        let concurrency = config_rs.concurrency.max(1);
        let config_for_pool = config_rs.clone();
        let (browser, handler_task, pool) = py
            .allow_threads(|| {
                runtime.block_on(async {
                    let (browser, mut handler) =
                        Browser::launch(cfg).await.map_err(BlazeError::from)?;
                    let task = tokio::spawn(async move {
                        while let Some(res) = handler.next().await {
                            if res.is_err() {
                                // Handler ended — browser will report errors on the next page op.
                                break;
                            }
                        }
                    });
                    let pool = PagePool::new(&browser, concurrency, &config_for_pool).await?;
                    Ok::<_, BlazeError>((browser, task, pool))
                })
            })
            .map_err(PyErr::from)?;

        let state = ClientState {
            runtime: runtime.clone(),
            browser: Arc::new(browser),
            pool,
            handler_task: parking_lot::Mutex::new(Some(handler_task)),
            config: parking_lot::RwLock::new(config_rs),
            closed: std::sync::atomic::AtomicBool::new(false),
        };

        Ok(Self {
            inner: Arc::new(state),
        })
    }

    /// Swap in a new config. Launch-only fields are validated Python-side
    /// before this call — we just replace atomically. Next fetch sees the
    /// new values.
    fn update_config(&self, config: &Bound<'_, PyAny>) -> PyResult<()> {
        self.check_open().map_err(PyErr::from)?;
        let new_cfg = parse_client_config(config).map_err(PyErr::from)?;
        log::debug!(target: "blazeweb::client", "update_config applied");
        *self.inner.config.write() = new_cfg;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Sync API — `py.allow_threads + block_on(do_*_inner(...))`.
    // -----------------------------------------------------------------------

    /// Fetch URL → RawRenderOutput (HTML only).
    fn fetch(
        &self,
        py: Python<'_>,
        url: String,
        per_call: &Bound<'_, PyAny>,
    ) -> PyResult<RawRenderOutput> {
        self.check_open().map_err(PyErr::from)?;
        let fetch_cfg = parse_fetch_config(per_call).map_err(PyErr::from)?;
        let state = self.inner.clone();
        let runtime = state.runtime.clone();
        py.allow_threads(move || runtime.block_on(do_fetch_inner(state, url, fetch_cfg)))
            .map_err(PyErr::from)
    }

    /// Screenshot URL → image bytes (png/jpeg/webp depending on per_shot.format).
    fn screenshot<'py>(
        &self,
        py: Python<'py>,
        url: String,
        per_shot: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        self.check_open().map_err(PyErr::from)?;
        let shot_cfg = parse_screenshot_config(per_shot).map_err(PyErr::from)?;
        let state = self.inner.clone();
        let runtime = state.runtime.clone();
        let png = py
            .allow_threads(move || runtime.block_on(do_screenshot_inner(state, url, shot_cfg)))
            .map_err(PyErr::from)?;
        Ok(PyBytes::new(py, &png))
    }

    /// Fetch URL → RawFetchOutput (HTML + image from one visit).
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
            runtime.block_on(do_fetch_all_inner(state, url, fetch_cfg, shot_cfg))
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
        log::debug!(
            target: "blazeweb::client",
            "batch dispatch: {} URLs, capture={capture}",
            urls.len()
        );
        let mode = parse_capture_mode(capture)?;
        let fetch_cfg = parse_fetch_config(per_call).map_err(PyErr::from)?;
        let state = self.inner.clone();
        let runtime = state.runtime.clone();

        let outputs = py
            .allow_threads(move || runtime.block_on(do_batch_inner(state, urls, fetch_cfg, mode)));

        let results = PyList::empty(py);
        for r in outputs {
            batch_result_to_py(py, r, mode, &results)?;
        }
        Ok(results)
    }

    /// Explicit shutdown. Closes pooled pages, drops the Browser (chromium
    /// quits), and joins the handler task.
    fn close(&self, py: Python<'_>) -> PyResult<()> {
        if self.inner.is_closed() {
            return Ok(());
        }
        log::info!(target: "blazeweb::client", "Client.close");
        self.inner
            .closed
            .store(true, std::sync::atomic::Ordering::Release);
        let state = self.inner.clone();
        let runtime = state.runtime.clone();
        py.allow_threads(move || {
            runtime.block_on(do_close_inner(state));
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

    // -----------------------------------------------------------------------
    // Async API — `pyo3_async_runtimes::tokio::future_into_py(do_*_inner(...))`.
    // Returns Python awaitables. The Python-side `AsyncClient` wraps these.
    // -----------------------------------------------------------------------

    /// Fetch URL → awaitable → RawRenderOutput.
    fn fetch_async<'py>(
        &self,
        py: Python<'py>,
        url: String,
        per_call: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.check_open().map_err(PyErr::from)?;
        let fetch_cfg = parse_fetch_config(per_call).map_err(PyErr::from)?;
        let state = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            do_fetch_inner(state, url, fetch_cfg)
                .await
                .map_err(PyErr::from)
        })
    }

    /// Screenshot URL → awaitable → bytes.
    fn screenshot_async<'py>(
        &self,
        py: Python<'py>,
        url: String,
        per_shot: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.check_open().map_err(PyErr::from)?;
        let shot_cfg = parse_screenshot_config(per_shot).map_err(PyErr::from)?;
        let state = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let bytes = do_screenshot_inner(state, url, shot_cfg)
                .await
                .map_err(PyErr::from)?;
            Python::with_gil(|py| -> PyResult<Py<PyBytes>> {
                Ok(PyBytes::new(py, &bytes).unbind())
            })
        })
    }

    /// Fetch URL → awaitable → RawFetchOutput (HTML + image from one visit).
    fn fetch_all_async<'py>(
        &self,
        py: Python<'py>,
        url: String,
        per_call: &Bound<'_, PyAny>,
        per_shot: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.check_open().map_err(PyErr::from)?;
        let fetch_cfg = parse_fetch_config(per_call).map_err(PyErr::from)?;
        let shot_cfg = parse_screenshot_config(per_shot).map_err(PyErr::from)?;
        let state = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            do_fetch_all_inner(state, url, fetch_cfg, shot_cfg)
                .await
                .map_err(PyErr::from)
        })
    }

    /// Batch URLs → awaitable → list. Same shape as sync `batch()`.
    fn batch_async<'py>(
        &self,
        py: Python<'py>,
        urls: Vec<String>,
        capture: &str,
        per_call: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.check_open().map_err(PyErr::from)?;
        log::debug!(
            target: "blazeweb::client",
            "batch_async dispatch: {} URLs, capture={capture}",
            urls.len()
        );
        let mode = parse_capture_mode(capture)?;
        let fetch_cfg = parse_fetch_config(per_call).map_err(PyErr::from)?;
        let state = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let outputs = do_batch_inner(state, urls, fetch_cfg, mode).await;
            Python::with_gil(|py| -> PyResult<Py<PyList>> {
                let results = PyList::empty(py);
                for r in outputs {
                    batch_result_to_py(py, r, mode, &results)?;
                }
                Ok(results.unbind())
            })
        })
    }

    /// Explicit shutdown → awaitable → None.
    fn close_async<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        if self.inner.is_closed() {
            // Already closed — return an immediately-resolved awaitable so
            // double-close doesn't error.
            return pyo3_async_runtimes::tokio::future_into_py(py, async { Ok(()) });
        }
        log::info!(target: "blazeweb::client", "Client.close_async");
        self.inner
            .closed
            .store(true, std::sync::atomic::Ordering::Release);
        let state = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            do_close_inner(state).await;
            Ok(())
        })
    }
}
