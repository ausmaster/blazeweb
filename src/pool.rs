//! Pre-warmed pool of chromium pages — pre-created at Client launch with base
//! config + listeners applied once. Each fetch navigates an existing page and
//! returns it to the pool, avoiding the ~50-150ms per-URL new_page tax.

use std::sync::Arc;

use chromiumoxide::cdp::browser_protocol::emulation::{
    SetDeviceMetricsOverrideParams, SetGeolocationOverrideParams, SetLocaleOverrideParams,
    SetScriptExecutionDisabledParams, SetTimezoneOverrideParams, UserAgentBrandVersion,
    UserAgentMetadata,
};
// EmulateNetworkConditionsParams is deprecated upstream in chromiumoxide
// 0.9 (CDP renamed it). The replacement isn't yet exported; allow the
// deprecation until the upstream type lands.
#[allow(deprecated)]
use chromiumoxide::cdp::browser_protocol::network::EmulateNetworkConditionsParams;
use chromiumoxide::cdp::browser_protocol::network::{
    BlockPattern, SetBlockedUrLsParams, SetCacheDisabledParams, SetExtraHttpHeadersParams,
    SetUserAgentOverrideParams,
};
use chromiumoxide::cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams;
use chromiumoxide::{Browser, Page};
use futures::StreamExt;
use parking_lot::Mutex;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::config::{CaptureConsoleLevel, ClientConfigRs, UserAgentMetadataRs};
use crate::error::{BlazeError, Result};
use crate::result::ConsoleMessageRs;

/// Name of the isolated JS world used for ``scripts.isolated_world``
/// registrations. Page JS cannot read or tamper with globals defined here.
const ISOLATED_WORLD_NAME: &str = "blazeweb_isolated";

/// Build a `Vec<BlockPattern>` for `Network.setBlockedURLs` from a slice of
/// URL pattern strings. URLPattern syntax (`*://*.doubleclick.net/*`),
/// case-sensitive matching enabled.
pub(crate) fn block_patterns(urls: &[String]) -> Vec<BlockPattern> {
    urls.iter()
        .map(|p| BlockPattern::new(p.clone(), true))
        .collect()
}

/// One pooled page + its persistent console-message and main-doc-status
/// collectors. Console messages flow in from ``Runtime.consoleAPICalled`` and
/// ``Runtime.exceptionThrown`` listeners spawned at page creation; drained
/// per-fetch by `engine::capture_page`.
pub struct PooledPage {
    pub page: Page,
    pub console_messages: Arc<Mutex<Vec<ConsoleMessageRs>>>,
    /// Latest main-doc HTTP status from Network.responseReceived — populated on
    /// response headers, well before DOMContentLoaded.
    pub main_status: Arc<Mutex<Option<u16>>>,
}

/// Pool sized to the Client's `concurrency`. `acquire()` returns page + permit
/// together; excess callers queue on the semaphore.
pub struct PagePool {
    pages: Mutex<Vec<PooledPage>>,
    sem: Arc<Semaphore>,
    #[allow(dead_code)]
    size: usize,
}

impl PagePool {
    /// Create `size` pages in parallel, each with base config applied and
    /// console/exception listeners wired up.
    pub async fn new(browser: &Browser, size: usize, base: &ClientConfigRs) -> Result<Arc<Self>> {
        let t0 = std::time::Instant::now();
        log::info!(target: "blazeweb::pool", "creating pool of {size} pages");
        let futs = (0..size).map(|_| create_pooled_page(browser, base));
        let created: Vec<PooledPage> = futures::future::try_join_all(futs).await?;
        log::info!(
            target: "blazeweb::pool",
            "pool of {size} pages ready in {:?}",
            t0.elapsed()
        );
        Ok(Arc::new(Self {
            pages: Mutex::new(created),
            sem: Arc::new(Semaphore::new(size)),
            size,
        }))
    }

    #[allow(dead_code)]
    pub fn size(&self) -> usize {
        self.size
    }

    /// Acquire a page (waits on Semaphore if pool is saturated).
    pub async fn acquire(self: &Arc<Self>) -> Result<PageGuard> {
        let t0 = std::time::Instant::now();
        let permit = self
            .sem
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| BlazeError::Internal(format!("pool sem: {e}")))?;
        let pooled = self
            .pages
            .lock()
            .pop()
            .expect("semaphore permitted but pool is empty");
        pooled.console_messages.lock().clear();
        *pooled.main_status.lock() = None;
        log::trace!(
            target: "blazeweb::pool",
            "acquired page (waited {:?}, pool available={})",
            t0.elapsed(),
            self.sem.available_permits()
        );
        Ok(PageGuard {
            page: Some(pooled),
            pool: self.clone(),
            _permit: permit,
        })
    }

    fn return_page(&self, p: PooledPage) {
        self.pages.lock().push(p);
        log::trace!(target: "blazeweb::pool", "page returned to pool");
    }

    /// Close every page in the pool. Call before dropping the Browser so we
    /// don't leak CDP targets.
    pub async fn close_all(&self) {
        let pages = std::mem::take(&mut *self.pages.lock());
        log::debug!(target: "blazeweb::pool", "closing {} pooled pages", pages.len());
        for p in pages {
            let _ = p.page.close().await;
        }
    }
}

/// RAII handle to a pooled page. Drops → page goes back to pool.
pub struct PageGuard {
    page: Option<PooledPage>,
    pool: Arc<PagePool>,
    _permit: OwnedSemaphorePermit,
}

impl PageGuard {
    pub fn page(&self) -> &Page {
        &self.page.as_ref().expect("guard drained").page
    }

    pub fn console_messages(&self) -> Arc<Mutex<Vec<ConsoleMessageRs>>> {
        self.page
            .as_ref()
            .expect("guard drained")
            .console_messages
            .clone()
    }

    /// Latest main-doc response status. None if no response has arrived yet.
    pub fn main_status(&self) -> Option<u16> {
        *self
            .page
            .as_ref()
            .expect("guard drained")
            .main_status
            .lock()
    }
}

impl Drop for PageGuard {
    fn drop(&mut self) {
        if let Some(p) = self.page.take() {
            self.pool.return_page(p);
        }
    }
}

/// Create one page, apply base config, wire up persistent listeners.
async fn create_pooled_page(browser: &Browser, base: &ClientConfigRs) -> Result<PooledPage> {
    let t0 = std::time::Instant::now();
    let page = browser
        .new_page("about:blank")
        .await
        .map_err(BlazeError::from)?;
    log::trace!(target: "blazeweb::pool", "new_page in {:?}", t0.elapsed());

    // Chrome's viewport defaults to 800×600 without an explicit override.
    page.execute(
        SetDeviceMetricsOverrideParams::builder()
            .width(base.viewport.width as i64)
            .height(base.viewport.height as i64)
            .device_scale_factor(base.viewport.device_scale_factor)
            .mobile(base.viewport.mobile)
            .build()
            .map_err(|e| BlazeError::Cdp(format!("metrics: {e}")))?,
    )
    .await?;

    if let Some(ua) = &base.network.user_agent {
        let mut builder = SetUserAgentOverrideParams::builder().user_agent(ua.clone());
        if let Some(meta) = &base.network.user_agent_metadata {
            builder = builder.user_agent_metadata(build_ua_metadata(meta)?);
        }
        page.execute(
            builder
                .build()
                .map_err(|e| BlazeError::Cdp(format!("UA: {e}")))?,
        )
        .await?;
    }

    if !base.network.extra_headers.is_empty() {
        let headers = chromiumoxide::cdp::browser_protocol::network::Headers::new(
            serde_json::to_value(&base.network.extra_headers)
                .map_err(|e| BlazeError::Internal(e.to_string()))?,
        );
        page.execute(SetExtraHttpHeadersParams::new(headers))
            .await?;
    }

    if !base.network.block_urls.is_empty() {
        log::trace!(
            target: "blazeweb::pool",
            "Network.setBlockedURLs ({} patterns)",
            base.network.block_urls.len()
        );
        page.execute(SetBlockedUrLsParams {
            url_patterns: Some(block_patterns(&base.network.block_urls)),
        })
        .await?;
    }

    if base.network.disable_cache {
        page.execute(SetCacheDisabledParams::new(true)).await?;
    }

    if base.network.offline
        || base.network.latency_ms.is_some()
        || base.network.download_bps.is_some()
        || base.network.upload_bps.is_some()
    {
        #[allow(deprecated)]
        page.execute(
            EmulateNetworkConditionsParams::builder()
                .offline(base.network.offline)
                .latency(base.network.latency_ms.unwrap_or(0.0))
                .download_throughput(base.network.download_bps.map(|x| x as f64).unwrap_or(-1.0))
                .upload_throughput(base.network.upload_bps.map(|x| x as f64).unwrap_or(-1.0))
                .build()
                .map_err(|e| BlazeError::Cdp(format!("net emu: {e}")))?,
        )
        .await?;
    }

    if let Some(locale) = &base.emulation.locale {
        page.execute(
            SetLocaleOverrideParams::builder()
                .locale(locale.clone())
                .build(),
        )
        .await?;
    }

    if let Some(tz) = &base.emulation.timezone {
        page.execute(
            SetTimezoneOverrideParams::builder()
                .timezone_id(tz.clone())
                .build()
                .map_err(|e| BlazeError::Cdp(format!("tz: {e}")))?,
        )
        .await?;
    }

    if let Some((lat, lon)) = base.emulation.geolocation {
        page.execute(
            SetGeolocationOverrideParams::builder()
                .latitude(lat)
                .longitude(lon)
                .accuracy(0.0)
                .build(),
        )
        .await?;
    }

    if !base.emulation.javascript_enabled {
        page.execute(SetScriptExecutionDisabledParams::new(true))
            .await?;
    }

    register_init_scripts(&page, base).await?;

    // Runtime domain must be enabled for ``consoleAPICalled`` events to fire.
    page.execute(chromiumoxide::cdp::js_protocol::runtime::EnableParams::default())
        .await?;

    // Persistent per-page listeners: structured console messages + main-doc
    // HTTP status. Level filter is captured here at page creation; runtime
    // updates to ``capture_console_level`` via update_config don't re-arm
    // these listeners.
    let console_messages: Arc<Mutex<Vec<ConsoleMessageRs>>> = Arc::new(Mutex::new(Vec::new()));
    let main_status: Arc<Mutex<Option<u16>>> = Arc::new(Mutex::new(None));
    let level = base.capture_console_level;
    {
        use chromiumoxide::cdp::browser_protocol::network::{EventResponseReceived, ResourceType};
        use chromiumoxide::cdp::browser_protocol::page::{
            EventJavascriptDialogOpening, HandleJavaScriptDialogParams,
        };
        use chromiumoxide::cdp::js_protocol::runtime::{
            ConsoleApiCalledType, EventConsoleApiCalled, EventExceptionThrown,
        };

        // Runtime.consoleAPICalled — every page-side ``console.*`` call.
        // Filtered by ``capture_console_level``: All keeps everything,
        // Warn drops log/info/debug/trace, Error drops everything except
        // error-level events.
        let cm_cl = console_messages.clone();
        let mut console_stream = page
            .event_listener::<EventConsoleApiCalled>()
            .await
            .map_err(BlazeError::from)?;
        tokio::spawn(async move {
            while let Some(evt) = console_stream.next().await {
                // We only surface six standard methods; ``dir``, ``table``,
                // ``startGroup`` etc. are dropped.
                let kind = match evt.r#type {
                    ConsoleApiCalledType::Log => "log",
                    ConsoleApiCalledType::Info => "info",
                    ConsoleApiCalledType::Warning => "warning",
                    ConsoleApiCalledType::Error => "error",
                    ConsoleApiCalledType::Debug => "debug",
                    ConsoleApiCalledType::Trace => "trace",
                    _ => continue,
                };
                let accept = match level {
                    CaptureConsoleLevel::All => true,
                    CaptureConsoleLevel::Warn => matches!(kind, "warning" | "error"),
                    CaptureConsoleLevel::Error => kind == "error",
                };
                if !accept {
                    continue;
                }
                // Stringify args: JSON strings come through unquoted, other
                // primitives via JSON repr, objects via `.description`.
                let text = evt
                    .args
                    .iter()
                    .map(|arg| {
                        arg.value
                            .as_ref()
                            .map(|v| match v {
                                serde_json::Value::String(s) => s.clone(),
                                other => other.to_string(),
                            })
                            .or_else(|| arg.description.clone())
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0);
                cm_cl.lock().push(ConsoleMessageRs {
                    kind: kind.to_string(),
                    text,
                    timestamp,
                });
            }
        });

        // Runtime.exceptionThrown — uncaught JS errors. Captured as
        // ConsoleMessage(type="error", ...) so they show up alongside
        // console.error calls.
        let cm_cl = console_messages.clone();
        let mut exc_stream = page
            .event_listener::<EventExceptionThrown>()
            .await
            .map_err(BlazeError::from)?;
        tokio::spawn(async move {
            while let Some(evt) = exc_stream.next().await {
                let det = &evt.exception_details;
                let text = det
                    .exception
                    .as_ref()
                    .and_then(|o| o.description.clone())
                    .unwrap_or_else(|| det.text.clone());
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0);
                cm_cl.lock().push(ConsoleMessageRs {
                    kind: "error".to_string(),
                    text,
                    timestamp,
                });
            }
        });

        // Overwrite on every Document response so redirects end with the
        // final status. Fires before DCL/load, much earlier than
        // wait_for_navigation_response would.
        let status_cl = main_status.clone();
        let mut resp_stream = page
            .event_listener::<EventResponseReceived>()
            .await
            .map_err(BlazeError::from)?;
        tokio::spawn(async move {
            while let Some(evt) = resp_stream.next().await {
                if matches!(evt.r#type, ResourceType::Document) {
                    *status_cl.lock() = Some(evt.response.status as u16);
                }
            }
        });

        // Page.javascriptDialogOpening — auto-dismiss native dialogs
        // (alert/confirm/prompt/beforeunload). Without this, any page
        // that calls these blocks the lifecycle event waiting for a UI
        // dismissal that never comes. Mirrors Playwright/Selenium defaults.
        let page_for_dialogs = page.clone();
        let mut dialog_stream = page
            .event_listener::<EventJavascriptDialogOpening>()
            .await
            .map_err(BlazeError::from)?;
        tokio::spawn(async move {
            while let Some(_evt) = dialog_stream.next().await {
                let _ = page_for_dialogs
                    .execute(
                        HandleJavaScriptDialogParams::builder()
                            .accept(false)
                            .build()
                            .expect("HandleJavaScriptDialogParams: accept is set"),
                    )
                    .await;
            }
        });
    }

    Ok(PooledPage {
        page,
        console_messages,
        main_status,
    })
}

/// Build a chromiumoxide ``UserAgentMetadata`` from our parsed config mirror.
fn build_ua_metadata(m: &UserAgentMetadataRs) -> Result<UserAgentMetadata> {
    let mut b = UserAgentMetadata::builder()
        .platform(m.platform.clone())
        .platform_version(m.platform_version.clone())
        .architecture(m.architecture.clone())
        .model(m.model.clone())
        .mobile(m.mobile);
    if let Some(brands) = &m.brands {
        for br in brands {
            b = b.brand(UserAgentBrandVersion::new(
                br.brand.clone(),
                br.version.clone(),
            ));
        }
    }
    if let Some(fvl) = &m.full_version_list {
        for br in fvl {
            b = b.full_version_list(UserAgentBrandVersion::new(
                br.brand.clone(),
                br.version.clone(),
            ));
        }
    }
    if let Some(bitness) = &m.bitness {
        b = b.bitness(bitness.clone());
    }
    if m.wow64 {
        b = b.wow64(true);
    }
    if let Some(ff) = &m.form_factors {
        b = b.form_factors(ff.clone());
    }
    b.build()
        .map_err(|e| BlazeError::Cdp(format!("UA metadata: {e}")))
}

/// Register all declarative init scripts via ``Page.addScriptToEvaluateOnNewDocument``.
/// Timing variants and URL scoping are implemented as source-wrapping; only
/// ``on_new_document`` and ``isolated_world`` map 1:1 to the CDP primitive.
async fn register_init_scripts(page: &Page, base: &ClientConfigRs) -> Result<()> {
    let s = &base.scripts;
    let total = s.on_new_document.len()
        + s.on_dom_content_loaded.len()
        + s.on_load.len()
        + s.isolated_world.len()
        + s.url_scoped.values().map(|v| v.len()).sum::<usize>();
    if total == 0 {
        return Ok(());
    }
    log::trace!(target: "blazeweb::pool", "registering {total} init scripts");

    for src in &s.on_new_document {
        page.execute(AddScriptToEvaluateOnNewDocumentParams::new(src.clone()))
            .await?;
    }

    for src in &s.on_dom_content_loaded {
        let wrapped =
            format!("document.addEventListener('DOMContentLoaded', function() {{ {src} }});");
        page.execute(AddScriptToEvaluateOnNewDocumentParams::new(wrapped))
            .await?;
    }

    for src in &s.on_load {
        let wrapped = format!("window.addEventListener('load', function() {{ {src} }});");
        page.execute(AddScriptToEvaluateOnNewDocumentParams::new(wrapped))
            .await?;
    }

    for src in &s.isolated_world {
        page.execute(
            AddScriptToEvaluateOnNewDocumentParams::builder()
                .source(src.clone())
                .world_name(ISOLATED_WORLD_NAME)
                .build()
                .map_err(|e| BlazeError::Cdp(format!("isolated script: {e}")))?,
        )
        .await?;
    }

    for (pattern, scripts) in &s.url_scoped {
        let pat_esc = js_escape_single_quoted(pattern);
        for src in scripts {
            let wrapped = format!("if (location.href.indexOf('{pat_esc}') !== -1) {{ {src} }}");
            page.execute(AddScriptToEvaluateOnNewDocumentParams::new(wrapped))
                .await?;
        }
    }

    Ok(())
}

/// Escape a string for safe embedding inside a JS single-quoted string literal.
fn js_escape_single_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out
}
