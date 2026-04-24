//! Interactive Session API — stateful drive-the-browser alternative to the
//! one-shot ``Client.fetch()`` flow. Native async throughout: every
//! CDP-touching method returns a Python awaitable via
//! ``pyo3_async_runtimes::tokio::future_into_py``.
//!
//! Public ergonomics (`async with client.session() as s: ...`) are layered
//! on top by a Python-side wrapper in ``python/blazeweb/session.py`` — this
//! Rust module exposes raw ``_SessionInner`` / ``_LiveElementInner``
//! PyClasses that the wrapper composes into user-facing ``Session`` and
//! ``LiveElement``.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
use chromiumoxide::cdp::browser_protocol::fetch::{
    ContinueRequestParams, EnableParams as FetchEnableParams, EventRequestPaused,
    FailRequestParams,
};
use chromiumoxide::cdp::browser_protocol::network::{
    ErrorReason, ResourceType, SetUserAgentOverrideParams,
};
use chromiumoxide::cdp::browser_protocol::page::{
    AddScriptToEvaluateOnNewDocumentParams, EventDomContentEventFired, EventLoadEventFired,
    NavigateParams,
};
use chromiumoxide::cdp::js_protocol::runtime::{
    CallFunctionOnParams, ConsoleApiCalledType, EnableParams as RuntimeEnableParams,
    EventConsoleApiCalled, RemoteObject,
};
use chromiumoxide::{Element as CoxElement, Page};
use futures::StreamExt;
use parking_lot::Mutex;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use tokio::sync::OwnedSemaphorePermit;

use crate::client::ClientState;
use crate::config::{ViewportRs, WaitUntil};
use crate::error::{BlazeError, Result};

// ---------------------------------------------------------------------------
// SessionConfigRs + parser
// ---------------------------------------------------------------------------

/// Per-session config overrides. Unset fields fall through to the Client's
/// base `ClientConfigRs`.
#[derive(Debug, Clone, Default)]
pub struct SessionConfigRs {
    pub viewport: Option<ViewportRs>,
    pub user_agent: Option<String>,
    /// Resource types to fail at the network layer. Values are the lowercase
    /// CDP resource names: ``image``, ``stylesheet``, ``font``, ``media``,
    /// ``xhr``, ``fetch``, ``document``, ``script``, ``websocket``, etc.
    pub block_resources: Vec<String>,
    /// URL substrings — any request whose URL contains one of these is failed.
    pub block_urls: Vec<String>,
    /// If true, fail navigation-type requests (main-frame URL commits).
    pub block_navigation: bool,
    /// True if the user explicitly passed any `block_*` kwarg (including
    /// empty / False). Drives whether Fetch.enable fires at open() time and
    /// whether runtime `block_*` setters are accepted.
    pub intercept_opt_in: bool,
}

/// Runtime-mutable subset of SessionConfigRs — the Fetch-interception task
/// reads this every request to decide continue vs fail.
#[derive(Debug, Clone, Default)]
struct BlockState {
    resources: Vec<String>,
    urls: Vec<String>,
    navigation: bool,
}

/// Parse the `kwargs` dict handed from `Client.session(**kwargs)`. Only the
/// small subset of keys supported in this task — task #22 extends.
pub fn parse_session_config(
    kwargs: Option<&Bound<'_, PyDict>>,
) -> Result<SessionConfigRs> {
    let mut out = SessionConfigRs::default();
    let Some(d) = kwargs else { return Ok(out) };

    if let Some(v) = d.get_item("viewport").map_err(to_internal)? {
        if !v.is_none() {
            out.viewport = Some(parse_viewport(&v)?);
        }
    }
    if let Some(v) = d.get_item("user_agent").map_err(to_internal)? {
        if !v.is_none() {
            out.user_agent = Some(v.extract().map_err(to_internal)?);
        }
    }
    if let Some(v) = d.get_item("block_resources").map_err(to_internal)? {
        out.intercept_opt_in = true;
        if !v.is_none() {
            out.block_resources = v.extract().map_err(to_internal)?;
        }
    }
    if let Some(v) = d.get_item("block_urls").map_err(to_internal)? {
        out.intercept_opt_in = true;
        if !v.is_none() {
            out.block_urls = v.extract().map_err(to_internal)?;
        }
    }
    if let Some(v) = d.get_item("block_navigation").map_err(to_internal)? {
        out.intercept_opt_in = true;
        if !v.is_none() {
            out.block_navigation = v.extract().map_err(to_internal)?;
        }
    }
    Ok(out)
}

/// ResourceType ↔ lowercase string. "image" ↔ ResourceType::Image, etc.
fn resource_type_name(rt: &ResourceType) -> String {
    // Debug serializes to PascalCase ("Image", "Stylesheet"); we lowercase
    // for user-friendly matching. ``Xhr`` → ``xhr`` works. Maintained
    // parallel to the CDP spec values.
    format!("{rt:?}").to_lowercase()
}

fn parse_viewport(v: &Bound<'_, PyAny>) -> Result<ViewportRs> {
    // Accept `(w, h)` tuple (the Client-level convention) or a dict matching
    // ViewportConfig.
    if let Ok(t) = v.downcast::<PyTuple>() {
        if t.len() == 2 {
            let w: u32 = t.get_item(0).map_err(to_internal)?.extract().map_err(to_internal)?;
            let h: u32 = t.get_item(1).map_err(to_internal)?.extract().map_err(to_internal)?;
            return Ok(ViewportRs {
                width: w, height: h, device_scale_factor: 1.0, mobile: false,
            });
        }
    }
    if let Ok(d) = v.downcast::<PyDict>() {
        let mut vp = ViewportRs { width: 1200, height: 800, device_scale_factor: 1.0, mobile: false };
        if let Some(x) = d.get_item("width").map_err(to_internal)? {
            vp.width = x.extract().map_err(to_internal)?;
        }
        if let Some(x) = d.get_item("height").map_err(to_internal)? {
            vp.height = x.extract().map_err(to_internal)?;
        }
        if let Some(x) = d.get_item("device_scale_factor").map_err(to_internal)? {
            vp.device_scale_factor = x.extract().map_err(to_internal)?;
        }
        if let Some(x) = d.get_item("mobile").map_err(to_internal)? {
            vp.mobile = x.extract().map_err(to_internal)?;
        }
        return Ok(vp);
    }
    Err(BlazeError::InvalidConfig(
        "viewport must be (w, h) tuple or dict".to_string(),
    ))
}

fn to_internal(e: PyErr) -> BlazeError {
    BlazeError::InvalidConfig(e.to_string())
}

// ---------------------------------------------------------------------------
// SessionInner — the PyClass backing `blazeweb.Session`
// ---------------------------------------------------------------------------

/// One captured console event: (level, concatenated-text, timestamp).
/// Emitted as a Python tuple so the dataclass on the Python side
/// (`blazeweb.ConsoleMessage`) can decode without crossing the FFI per
/// field.
pub type ConsoleMessageRs = (String, String, f64);

/// Rust-side handle that does the actual CDP work. Exposed as
/// ``blazeweb._blazeweb._SessionInner``; the public ``blazeweb.Session``
/// Python class delegates to this.
#[pyclass(name = "_SessionInner")]
pub struct SessionInner {
    state: Arc<ClientState>,
    config: SessionConfigRs,
    /// The live chromium Page. `None` before `open()` / after `close()`.
    page: Arc<Mutex<Option<Page>>>,
    /// Held for the session's lifetime to cap concurrency.
    permit: Arc<Mutex<Option<OwnedSemaphorePermit>>>,
    /// Cached so `url` can be a sync property without a CDP round-trip.
    last_url: Arc<Mutex<String>>,
    /// Console events captured since open / last clear. Populated by a
    /// background tokio task listening on `Runtime.consoleAPICalled`.
    console: Arc<Mutex<Vec<ConsoleMessageRs>>>,
    /// Runtime-mutable request blocking state. Read by the Fetch-interception
    /// task on every paused request.
    block_state: Arc<Mutex<BlockState>>,
    /// Whether Fetch interception was opted in at `open()` time. Runtime
    /// block-* setters only take effect when this is true; otherwise they
    /// raise so users get a clear "opt in at session creation" error.
    intercept_enabled: Arc<AtomicBool>,
    closed: Arc<AtomicBool>,
}

impl SessionInner {
    pub fn new(state: Arc<ClientState>, config: SessionConfigRs) -> Self {
        let block_state = BlockState {
            resources: config.block_resources.clone(),
            urls: config.block_urls.clone(),
            navigation: config.block_navigation,
        };
        Self {
            state,
            config,
            page: Arc::new(Mutex::new(None)),
            permit: Arc::new(Mutex::new(None)),
            last_url: Arc::new(Mutex::new(String::new())),
            console: Arc::new(Mutex::new(Vec::new())),
            block_state: Arc::new(Mutex::new(block_state)),
            intercept_enabled: Arc::new(AtomicBool::new(false)),
            closed: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Snapshot the page handle for use inside an async future. Returns an
    /// error if the session isn't open.
    fn page_clone(page_slot: &Arc<Mutex<Option<Page>>>) -> Result<Page> {
        page_slot
            .lock()
            .as_ref()
            .cloned()
            .ok_or_else(|| BlazeError::Internal("session is not open".to_string()))
    }
}

/// Convert one CDP `RemoteObject` to its user-visible string form. Strings
/// come through directly; other JSON values get `serde_json` stringified;
/// objects / arrays fall back to the description (`"[object Object]"` etc.).
fn remote_object_to_string(obj: &RemoteObject) -> String {
    if let Some(v) = &obj.value {
        if let Some(s) = v.as_str() {
            return s.to_string();
        }
        return v.to_string();
    }
    obj.description.clone().unwrap_or_else(|| "undefined".to_string())
}

fn console_type_name(t: &ConsoleApiCalledType) -> &'static str {
    match t {
        ConsoleApiCalledType::Log => "log",
        ConsoleApiCalledType::Debug => "debug",
        ConsoleApiCalledType::Info => "info",
        ConsoleApiCalledType::Error => "error",
        ConsoleApiCalledType::Warning => "warning",
        ConsoleApiCalledType::Dir => "dir",
        ConsoleApiCalledType::Dirxml => "dirxml",
        ConsoleApiCalledType::Table => "table",
        ConsoleApiCalledType::Trace => "trace",
        ConsoleApiCalledType::Clear => "clear",
        ConsoleApiCalledType::StartGroup => "startGroup",
        ConsoleApiCalledType::StartGroupCollapsed => "startGroupCollapsed",
        ConsoleApiCalledType::EndGroup => "endGroup",
        ConsoleApiCalledType::Assert => "assert",
        ConsoleApiCalledType::Profile => "profile",
        ConsoleApiCalledType::ProfileEnd => "profileEnd",
        ConsoleApiCalledType::Count => "count",
        ConsoleApiCalledType::TimeEnd => "timeEnd",
    }
}

#[pymethods]
impl SessionInner {
    /// Allocate a fresh chromium page, acquire a pool permit, and apply
    /// session config. Called from Python-side ``Session.__aenter__``.
    fn open<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let state = self.state.clone();
        let config = self.config.clone();
        let page_slot = self.page.clone();
        let permit_slot = self.permit.clone();
        let console_slot = self.console.clone();
        let block_state_slot = self.block_state.clone();
        let intercept_enabled = self.intercept_enabled.clone();
        let closed = self.closed.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            if closed.load(Ordering::Acquire) {
                return Err(PyErr::from(BlazeError::Internal(
                    "session was closed; open a new one".to_string(),
                )));
            }
            if page_slot.lock().is_some() {
                return Err(PyErr::from(BlazeError::Internal(
                    "session is already open".to_string(),
                )));
            }

            // Acquire the Client's concurrency permit async-ly. Queues here if
            // the cap is saturated — no thread blocking.
            let sem = state.pool.semaphore();
            let permit = sem
                .acquire_owned()
                .await
                .map_err(|e| PyErr::from(BlazeError::Internal(format!("sem: {e}"))))?;

            let page = state
                .browser
                .new_page("about:blank")
                .await
                .map_err(|e| PyErr::from(BlazeError::from(e)))?;

            // Apply viewport: session override > client base.
            let base = state.config.read().clone();
            let vp = config.viewport.clone().unwrap_or(base.viewport);
            page.execute(
                SetDeviceMetricsOverrideParams::builder()
                    .width(vp.width as i64)
                    .height(vp.height as i64)
                    .device_scale_factor(vp.device_scale_factor)
                    .mobile(vp.mobile)
                    .build()
                    .map_err(|e| BlazeError::Cdp(format!("viewport: {e}")))?,
            )
            .await
            .map_err(|e| PyErr::from(BlazeError::from(e)))?;

            // Apply UA: session override > client base.
            if let Some(ua) = config.user_agent.clone().or(base.network.user_agent.clone()) {
                page.execute(
                    SetUserAgentOverrideParams::builder()
                        .user_agent(ua)
                        .build()
                        .map_err(|e| BlazeError::Cdp(format!("UA: {e}")))?,
                )
                .await
                .map_err(|e| PyErr::from(BlazeError::from(e)))?;
            }

            // Enable Runtime domain so console.* events fire. Note this is the
            // tripwire that injects `window.cdc_*` — acceptable for the
            // interactive-drive use case (DOMino is the observer, not the
            // target of detection).
            page.execute(RuntimeEnableParams::default())
                .await
                .map_err(|e| PyErr::from(BlazeError::from(e)))?;

            // Console capture — all levels (log/warn/error/info/debug/...).
            let console_buf = console_slot.clone();
            let mut stream = page
                .event_listener::<EventConsoleApiCalled>()
                .await
                .map_err(|e| PyErr::from(BlazeError::from(e)))?;
            tokio::spawn(async move {
                while let Some(evt) = stream.next().await {
                    let text = evt
                        .args
                        .iter()
                        .map(remote_object_to_string)
                        .collect::<Vec<_>>()
                        .join(" ");
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs_f64())
                        .unwrap_or(0.0);
                    console_buf
                        .lock()
                        .push((console_type_name(&evt.r#type).to_string(), text, ts));
                }
            });

            // Fetch-domain interception: opt-in if user passed any block-*
            // kwarg at session creation (even empty / False).
            let intercept = config.intercept_opt_in;
            if intercept {
                page.execute(FetchEnableParams::default())
                    .await
                    .map_err(|e| PyErr::from(BlazeError::from(e)))?;

                let block = block_state_slot.clone();
                let page_for_task = page.clone();
                let mut paused_stream = page
                    .event_listener::<EventRequestPaused>()
                    .await
                    .map_err(|e| PyErr::from(BlazeError::from(e)))?;
                tokio::spawn(async move {
                    while let Some(evt) = paused_stream.next().await {
                        let should_block = {
                            let s = block.lock();
                            let rtype = resource_type_name(&evt.resource_type);
                            let hit_resource = s
                                .resources
                                .iter()
                                .any(|r| r.eq_ignore_ascii_case(&rtype));
                            let hit_url = s
                                .urls
                                .iter()
                                .any(|p| evt.request.url.contains(p.as_str()));
                            let hit_nav = s.navigation
                                && matches!(evt.resource_type, ResourceType::Document);
                            hit_resource || hit_url || hit_nav
                        };
                        let err = if should_block {
                            page_for_task
                                .execute(FailRequestParams::new(
                                    evt.request_id.clone(),
                                    ErrorReason::Aborted,
                                ))
                                .await
                                .err()
                                .map(|e| e.to_string())
                        } else {
                            page_for_task
                                .execute(ContinueRequestParams::new(evt.request_id.clone()))
                                .await
                                .err()
                                .map(|e| e.to_string())
                        };
                        if let Some(e) = err {
                            log::trace!(
                                target: "blazeweb::session",
                                "fetch interception response failed: {e}"
                            );
                        }
                    }
                });
            }

            *page_slot.lock() = Some(page);
            *permit_slot.lock() = Some(permit);
            intercept_enabled.store(intercept, Ordering::Release);
            log::debug!(
                target: "blazeweb::session",
                "session opened (intercept={intercept})"
            );
            Ok(())
        })
    }

    /// Close the page and release the concurrency permit. Idempotent.
    fn close<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let page_slot = self.page.clone();
        let permit_slot = self.permit.clone();
        let closed = self.closed.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            if closed.swap(true, Ordering::AcqRel) {
                return Ok(());
            }
            let page = page_slot.lock().take();
            if let Some(page) = page {
                let _ = page.close().await;
            }
            *permit_slot.lock() = None;
            log::debug!(target: "blazeweb::session", "session closed");
            Ok(())
        })
    }

    /// Navigate to ``url``. Waits for the lifecycle event chosen by
    /// ``wait_until`` (``"load"`` | ``"domcontentloaded"``) up to
    /// ``timeout_ms``. ``referer`` injects the Referer header for this
    /// navigation only.
    #[pyo3(signature = (url, *, timeout_ms=30_000, referer=None, wait_until="load"))]
    fn goto<'py>(
        &self,
        py: Python<'py>,
        url: String,
        timeout_ms: u64,
        referer: Option<String>,
        wait_until: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        let page_slot = self.page.clone();
        let last_url = self.last_url.clone();
        let wait_until = match wait_until {
            "load" => WaitUntil::Load,
            "domcontentloaded" | "dcl" => WaitUntil::DomContentLoaded,
            other => {
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "unknown wait_until {other:?}; expected 'load' or 'domcontentloaded'"
                )))
            }
        };

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let page = Self::page_clone(&page_slot).map_err(PyErr::from)?;

            // Subscribe BEFORE navigation to avoid the race where the lifecycle
            // event fires before we start listening. Pattern from engine::capture_page.
            let mut dcl_stream = page
                .event_listener::<EventDomContentEventFired>()
                .await
                .map_err(|e| PyErr::from(BlazeError::from(e)))?;
            let mut load_stream = page
                .event_listener::<EventLoadEventFired>()
                .await
                .map_err(|e| PyErr::from(BlazeError::from(e)))?;

            let mut params = NavigateParams::new(url.clone());
            params.referrer = referer;

            let nav = tokio::time::timeout(
                std::time::Duration::from_millis(timeout_ms),
                page.goto(params),
            )
            .await;
            match nav {
                Err(_) => {
                    return Err(PyErr::from(BlazeError::Internal(format!(
                        "goto timeout after {timeout_ms}ms (navigation did not commit)"
                    ))));
                }
                Ok(Err(e)) => return Err(PyErr::from(BlazeError::from(e))),
                Ok(Ok(_)) => {}
            }

            // Wait for the chosen lifecycle event — capped by the same timeout.
            let wait_fut = async {
                match wait_until {
                    WaitUntil::DomContentLoaded => {
                        dcl_stream.next().await;
                    }
                    WaitUntil::Load => {
                        load_stream.next().await;
                    }
                }
            };
            let _ = tokio::time::timeout(
                std::time::Duration::from_millis(timeout_ms),
                wait_fut,
            )
            .await;
            // Cache URL for sync getter.
            if let Ok(Some(current)) = page.url().await {
                *last_url.lock() = current;
            } else {
                *last_url.lock() = url;
            }
            Ok(())
        })
    }

    /// Return the page's current rendered HTML (post-JS).
    fn content<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let page_slot = self.page.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let page = Self::page_clone(&page_slot).map_err(PyErr::from)?;
            page.content()
                .await
                .map_err(|e| PyErr::from(BlazeError::from(e)))
        })
    }

    /// Sync URL getter. Cached after each navigation to avoid a CDP round-trip.
    #[getter]
    fn url(&self) -> String {
        self.last_url.lock().clone()
    }

    /// Async sleep — mirrors Playwright's ``page.wait_for_timeout(ms)``.
    fn sleep<'py>(&self, py: Python<'py>, ms: u64) -> PyResult<Bound<'py, PyAny>> {
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
            Ok(())
        })
    }

    /// Register a JavaScript source to run before any page script on every
    /// navigation (wraps CDP ``Page.addScriptToEvaluateOnNewDocument``).
    fn add_init_script<'py>(
        &self,
        py: Python<'py>,
        source: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let page_slot = self.page.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let page = Self::page_clone(&page_slot).map_err(PyErr::from)?;
            page.execute(AddScriptToEvaluateOnNewDocumentParams::new(source))
                .await
                .map_err(|e| PyErr::from(BlazeError::from(e)))?;
            Ok(())
        })
    }

    /// Evaluate a JS expression in the page's main world. Returns the JSON-
    /// serialized result (dict / list / str / int / float / bool / None).
    /// Non-JSON-serializable values (functions, live DOM nodes) come back
    /// as ``None``; wrap in a serializable expression to get structure.
    fn evaluate<'py>(&self, py: Python<'py>, js: String) -> PyResult<Bound<'py, PyAny>> {
        let page_slot = self.page.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let page = Self::page_clone(&page_slot).map_err(PyErr::from)?;
            let result = page
                .evaluate(js)
                .await
                .map_err(|e| PyErr::from(BlazeError::from(e)))?;
            Python::with_gil(|py| -> PyResult<PyObject> {
                match result.value() {
                    Some(v) => {
                        let bound = pythonize::pythonize(py, v).map_err(|e| {
                            pyo3::exceptions::PyRuntimeError::new_err(format!(
                                "failed to convert JS return value: {e}"
                            ))
                        })?;
                        Ok(bound.into())
                    }
                    None => Ok(py.None()),
                }
            })
        })
    }

    /// Snapshot the current console buffer. Safe to iterate; the background
    /// listener pushes new entries to the underlying Vec which we clone here.
    #[getter]
    fn console_messages(&self) -> Vec<ConsoleMessageRs> {
        self.console.lock().clone()
    }

    /// Reset the console buffer.
    fn clear_console(&self) {
        self.console.lock().clear();
    }

    /// First element matching ``selector`` (CSS), or ``None``.
    fn query<'py>(&self, py: Python<'py>, selector: String) -> PyResult<Bound<'py, PyAny>> {
        let page_slot = self.page.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let page = Self::page_clone(&page_slot).map_err(PyErr::from)?;
            // find_element errors when nothing matches — we map that to None.
            match page.find_element(selector).await {
                Ok(el) => Python::with_gil(|py| {
                    let inner = LiveElementInner::new(el, page.clone());
                    let obj = Py::new(py, inner)?;
                    Ok(obj.into_any())
                }),
                Err(_) => Python::with_gil(|py| Ok(py.None())),
            }
        })
    }

    /// All elements matching ``selector`` (CSS). Empty list when none match.
    fn query_all<'py>(
        &self,
        py: Python<'py>,
        selector: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let page_slot = self.page.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let page = Self::page_clone(&page_slot).map_err(PyErr::from)?;
            let elements = page.find_elements(selector).await.unwrap_or_default();
            Python::with_gil(|py| -> PyResult<PyObject> {
                let list = pyo3::types::PyList::empty(py);
                for el in elements {
                    let inner = LiveElementInner::new(el, page.clone());
                    list.append(Py::new(py, inner)?)?;
                }
                Ok(list.into())
            })
        })
    }

    /// Update the set of resource types to block. Requires the session to
    /// have been opened with Fetch interception opted-in (i.e. at least
    /// one of ``block_resources`` / ``block_urls`` / ``block_navigation``
    /// was non-empty at creation).
    fn block_resources<'py>(
        &self,
        py: Python<'py>,
        types: Vec<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let block = self.block_state.clone();
        let enabled = self.intercept_enabled.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            if !enabled.load(Ordering::Acquire) {
                return Err(PyErr::from(BlazeError::Internal(
                    "session was opened without Fetch interception — pass \
                     block_resources/block_urls/block_navigation at \
                     client.session(...) to enable runtime blocking"
                        .to_string(),
                )));
            }
            block.lock().resources = types;
            Ok(())
        })
    }

    /// Update the URL substring block list.
    fn block_urls<'py>(
        &self,
        py: Python<'py>,
        patterns: Vec<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let block = self.block_state.clone();
        let enabled = self.intercept_enabled.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            if !enabled.load(Ordering::Acquire) {
                return Err(PyErr::from(BlazeError::Internal(
                    "session was opened without Fetch interception".to_string(),
                )));
            }
            block.lock().urls = patterns;
            Ok(())
        })
    }

    /// Toggle navigation blocking — when true, main-frame Document requests
    /// are failed. Useful during click loops to trap links that would
    /// otherwise follow mid-scan.
    fn block_navigation<'py>(
        &self,
        py: Python<'py>,
        enabled: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let block = self.block_state.clone();
        let intercept = self.intercept_enabled.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            if !intercept.load(Ordering::Acquire) {
                return Err(PyErr::from(BlazeError::Internal(
                    "session was opened without Fetch interception".to_string(),
                )));
            }
            block.lock().navigation = enabled;
            Ok(())
        })
    }

    /// Poll for an element matching ``selector`` until it appears or
    /// ``timeout_ms`` elapses. Raises ``TimeoutError`` (Python) on expiry.
    /// Polling backs off exponentially from 20 ms to 200 ms.
    #[pyo3(signature = (selector, *, timeout_ms=5_000))]
    fn wait_for_selector<'py>(
        &self,
        py: Python<'py>,
        selector: String,
        timeout_ms: u64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let page_slot = self.page.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let page = Self::page_clone(&page_slot).map_err(PyErr::from)?;
            let deadline = std::time::Instant::now()
                + std::time::Duration::from_millis(timeout_ms);
            let mut delay = std::time::Duration::from_millis(20);
            loop {
                match page.find_element(selector.clone()).await {
                    Ok(el) => {
                        return Python::with_gil(|py| {
                            let inner = LiveElementInner::new(el, page.clone());
                            let obj = Py::new(py, inner)?;
                            Ok(obj.into_any())
                        });
                    }
                    Err(_) => {
                        if std::time::Instant::now() >= deadline {
                            return Err(PyErr::new::<pyo3::exceptions::PyTimeoutError, _>(
                                format!(
                                    "wait_for_selector({selector:?}) timed out after {timeout_ms}ms"
                                ),
                            ));
                        }
                        tokio::time::sleep(delay).await;
                        delay = (delay * 2).min(std::time::Duration::from_millis(200));
                    }
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// LiveElementInner — the PyClass backing `blazeweb.LiveElement`
// ---------------------------------------------------------------------------

/// Rust-side handle for a live DOM element. Exposed as
/// ``blazeweb._blazeweb._LiveElementInner``. Distinct from the existing
/// static-snapshot ``Element`` — this one is bound to a Session and can
/// be clicked / filled / evaluated on.
///
/// Holds both the ``Element`` (for its public methods) and the parent
/// ``Page`` (so we can issue raw CDP commands like ``Runtime.callFunctionOn``
/// with ``returnByValue: true`` — which chromiumoxide's
/// ``Element::call_js_fn`` doesn't expose). Element is wrapped in ``Arc``
/// because chromiumoxide's ``Element`` itself isn't ``Clone``.
#[pyclass(name = "_LiveElementInner")]
pub struct LiveElementInner {
    element: Arc<CoxElement>,
    page: Page,
}

impl LiveElementInner {
    fn new(el: CoxElement, page: Page) -> Self {
        Self { element: Arc::new(el), page }
    }
}

#[pymethods]
impl LiveElementInner {
    /// Dispatch a trusted mouse click. Scrolls into view first. ``timeout_ms``
    /// wraps the whole operation.
    #[pyo3(signature = (*, timeout_ms=5_000))]
    fn click<'py>(&self, py: Python<'py>, timeout_ms: u64) -> PyResult<Bound<'py, PyAny>> {
        let el = self.element.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let fut = el.click();
            tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), fut)
                .await
                .map_err(|_| PyErr::from(BlazeError::Internal(format!(
                    "click timeout after {timeout_ms}ms"
                ))))?
                .map_err(|e| PyErr::from(BlazeError::from(e)))?;
            Ok(())
        })
    }

    /// Clear the element's value and set it to ``text``. Fires ``input``
    /// and ``change`` events (bubbling) so framework reactivity sees it.
    /// Matches Playwright ``element.fill()`` semantics.
    #[pyo3(signature = (text, *, timeout_ms=5_000))]
    fn fill<'py>(
        &self,
        py: Python<'py>,
        text: String,
        timeout_ms: u64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let el = self.element.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let value_js = serde_json::to_string(&text)
                .map_err(|e| PyErr::from(BlazeError::Internal(format!("fill arg: {e}"))))?;
            let fn_src = format!(
                "function() {{ \
                    this.focus(); \
                    this.value = {value_js}; \
                    this.dispatchEvent(new Event('input', {{bubbles: true}})); \
                    this.dispatchEvent(new Event('change', {{bubbles: true}})); \
                }}"
            );
            let fut = el.call_js_fn(fn_src, false);
            tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), fut)
                .await
                .map_err(|_| PyErr::from(BlazeError::Internal(format!(
                    "fill timeout after {timeout_ms}ms"
                ))))?
                .map_err(|e| PyErr::from(BlazeError::from(e)))?;
            Ok(())
        })
    }

    /// Rendered text content (matches DOM ``.innerText``). Returns the
    /// empty string when the element has no text or the lookup fails.
    fn inner_text<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let el = self.element.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let text = el
                .inner_text()
                .await
                .map_err(|e| PyErr::from(BlazeError::from(e)))?
                .unwrap_or_default();
            Ok(text)
        })
    }

    /// HTML attribute value, or ``None`` if the attribute is unset.
    fn get_attribute<'py>(
        &self,
        py: Python<'py>,
        name: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let el = self.element.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let val = el
                .attribute(name)
                .await
                .map_err(|e| PyErr::from(BlazeError::from(e)))?;
            Python::with_gil(|py| match val {
                Some(s) => Ok(s.into_pyobject(py)?.into_any().unbind()),
                None => Ok(py.None()),
            })
        })
    }

    /// Evaluate a JS function with this element bound as ``this``.
    /// Pass the function DECLARATION (e.g. ``"function() { return
    /// this.outerHTML; }"`` or ``"function() { return {a: this.id}; }"``).
    /// Return value must be JSON-compatible.
    ///
    /// Uses CDP's ``Runtime.callFunctionOn`` with ``returnByValue: true``
    /// directly (matches Playwright's
    /// ``CRExecutionContext.evaluateWithArguments`` in
    /// ``packages/playwright-core/src/server/chromium/crExecutionContext.ts``).
    /// chromiumoxide's ``Element::call_js_fn`` hardcodes
    /// ``returnByValue: false``, so we route around it via
    /// ``page.execute(CallFunctionOnParams)``.
    fn evaluate<'py>(&self, py: Python<'py>, js: String) -> PyResult<Bound<'py, PyAny>> {
        let el = self.element.clone();
        let page = self.page.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let params = CallFunctionOnParams::builder()
                .object_id(el.remote_object_id.clone())
                .function_declaration(js)
                .return_by_value(true)
                .await_promise(true)
                .build()
                .map_err(|e| PyErr::from(BlazeError::Cdp(format!("callFn: {e}"))))?;
            let resp = page
                .execute(params)
                .await
                .map_err(|e| PyErr::from(BlazeError::from(e)))?;
            if let Some(exc) = &resp.result.exception_details {
                let msg = exc
                    .exception
                    .as_ref()
                    .and_then(|o| o.description.clone())
                    .unwrap_or_else(|| exc.text.clone());
                return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "JS exception in element.evaluate: {msg}"
                )));
            }
            Python::with_gil(|py| -> PyResult<PyObject> {
                match resp.result.result.value.as_ref() {
                    Some(v) => {
                        let bound = pythonize::pythonize(py, v).map_err(|e| {
                            pyo3::exceptions::PyRuntimeError::new_err(format!(
                                "failed to convert element.evaluate return value: {e}"
                            ))
                        })?;
                        Ok(bound.into())
                    }
                    None => Ok(py.None()),
                }
            })
        })
    }
}
