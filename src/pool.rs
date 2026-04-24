//! Pre-warmed pool of chromium Pages — the core perf optimization.
//!
//! Creating a new CDP target per URL costs ~50-150ms of handler-task time
//! (new_page + SetDeviceMetrics + event-listener subscription). When driving
//! hundreds of URLs, that tax dominates throughput. Instead we pre-create
//! `concurrency` pages at Client launch, apply base config + register console
//! listeners once, then each fetch just navigates an existing page and returns
//! it to the pool — no per-URL setup, no per-URL close.

use std::sync::Arc;

use chromiumoxide::cdp::browser_protocol::emulation::{
    SetDeviceMetricsOverrideParams, SetGeolocationOverrideParams, SetLocaleOverrideParams,
    SetScriptExecutionDisabledParams, SetTimezoneOverrideParams,
};
use chromiumoxide::cdp::browser_protocol::network::{
    BlockPattern, EmulateNetworkConditionsParams, SetBlockedUrLsParams, SetCacheDisabledParams,
    SetExtraHttpHeadersParams, SetUserAgentOverrideParams,
};
use chromiumoxide::{Browser, Page};
use futures::StreamExt;
use parking_lot::Mutex;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::config::ClientConfigRs;
use crate::error::{BlazeError, Result};

/// One pooled page + its persistent per-page console-error collector and
/// main-document status tracker.
pub struct PooledPage {
    pub page: Page,
    pub errors: Arc<Mutex<Vec<String>>>,
    /// Status of the most recent main-doc response. Filled by a Network.responseReceived
    /// listener — fires as soon as response headers arrive, BEFORE DOMContentLoaded.
    /// Needed because `wait_for_navigation_response()` can block until load event.
    pub main_status: Arc<Mutex<Option<u16>>>,
}

/// A pre-warmed set of Pages, sized to the Client's concurrency. `acquire()`
/// hands out (page, semaphore permit) together — capping in-flight pages at
/// pool size is baked in, no separate rate-limit needed upstream.
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
        pooled.errors.lock().clear();
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

    pub fn errors(&self) -> Arc<Mutex<Vec<String>>> {
        self.page.as_ref().expect("guard drained").errors.clone()
    }

    /// The status code of the most-recent main-document response (filled by
    /// our Network.responseReceived listener). None if no response arrived yet.
    pub fn main_status(&self) -> Option<u16> {
        *self.page.as_ref().expect("guard drained").main_status.lock()
    }
}

impl Drop for PageGuard {
    fn drop(&mut self) {
        if let Some(p) = self.page.take() {
            self.pool.return_page(p);
        }
    }
}

/// Create ONE page, apply every base-config CDP knob, wire up console /
/// exception listeners into a per-page error Vec.
async fn create_pooled_page(browser: &Browser, base: &ClientConfigRs) -> Result<PooledPage> {
    let t0 = std::time::Instant::now();
    let page = browser
        .new_page("about:blank")
        .await
        .map_err(BlazeError::from)?;
    log::trace!(
        target: "blazeweb::pool",
        "new_page(about:blank) in {:?}",
        t0.elapsed()
    );

    // Viewport — always apply (Chrome's default without this is 800×600).
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
        page.execute(
            SetUserAgentOverrideParams::builder()
                .user_agent(ua.clone())
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
        page.execute(SetExtraHttpHeadersParams::new(headers)).await?;
    }

    if !base.network.block_urls.is_empty() {
        log::trace!(
            target: "blazeweb::pool",
            "Network.setBlockedURLs ({} patterns)",
            base.network.block_urls.len()
        );
        // Each user pattern becomes a BlockPattern with block=true. URLPattern
        // syntax — e.g. `*://*:*/*.css` — not to be confused with legacy glob.
        let patterns: Vec<BlockPattern> = base
            .network
            .block_urls
            .iter()
            .map(|p| BlockPattern::new(p.clone(), true))
            .collect();
        page.execute(SetBlockedUrLsParams {
            url_patterns: Some(patterns),
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

    // --- Persistent per-page error + main-doc-status collectors ---
    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let main_status: Arc<Mutex<Option<u16>>> = Arc::new(Mutex::new(None));
    {
        use chromiumoxide::cdp::browser_protocol::log::EventEntryAdded;
        use chromiumoxide::cdp::browser_protocol::network::{
            EventResponseReceived, ResourceType,
        };
        use chromiumoxide::cdp::js_protocol::runtime::EventExceptionThrown;

        let errors_cl = errors.clone();
        let mut console_stream = page
            .event_listener::<EventEntryAdded>()
            .await
            .map_err(BlazeError::from)?;
        tokio::spawn(async move {
            while let Some(evt) = console_stream.next().await {
                let entry = &evt.entry;
                if matches!(
                    entry.level,
                    chromiumoxide::cdp::browser_protocol::log::LogEntryLevel::Error
                ) {
                    errors_cl.lock().push(entry.text.clone());
                }
            }
        });

        let errors_cl = errors.clone();
        let mut exc_stream = page
            .event_listener::<EventExceptionThrown>()
            .await
            .map_err(BlazeError::from)?;
        tokio::spawn(async move {
            while let Some(evt) = exc_stream.next().await {
                let det = &evt.exception_details;
                let msg = det
                    .exception
                    .as_ref()
                    .and_then(|o| o.description.clone())
                    .unwrap_or_else(|| det.text.clone());
                errors_cl.lock().push(msg);
            }
        });

        // Main-doc status via responseReceived. Fires as soon as response
        // headers arrive — before DCL, load, or anything else. We keep
        // overwriting on each Document-type response so redirects end up
        // with the final status.
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
    }

    Ok(PooledPage { page, errors, main_status })
}
