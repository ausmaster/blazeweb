//! Core CDP dance: launch a page, apply config, navigate, capture, return.
//!
//! Driven by `Client` via tokio. All async, all inside `py.allow_threads()` —
//! the GIL is released for the entire flight.

use std::sync::Arc;
use std::time::{Duration, Instant};

use chromiumoxide::cdp::browser_protocol::emulation::{
    SetDeviceMetricsOverrideParams, SetGeolocationOverrideParams, SetLocaleOverrideParams,
    SetScriptExecutionDisabledParams, SetTimezoneOverrideParams,
};
use chromiumoxide::cdp::browser_protocol::network::{
    EmulateNetworkConditionsParams, SetCacheDisabledParams, SetExtraHttpHeadersParams,
    SetUserAgentOverrideParams,
};
use chromiumoxide::cdp::browser_protocol::page::{
    CaptureScreenshotFormat, CaptureScreenshotParams,
};
use chromiumoxide::Browser;
use futures::StreamExt;
use parking_lot::Mutex;

use crate::config::{ClientConfigRs, FetchConfigRs, ScreenshotConfigRs};
use crate::error::{BlazeError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureMode {
    Html,
    Png,
    Both,
}

pub struct CaptureOutput {
    pub html: Option<String>,
    pub png: Option<Vec<u8>>,
    pub errors: Vec<String>,
    pub final_url: String,
    pub status_code: u16,
    pub elapsed_s: f64,
}

/// Capture a single URL with the given config. All CDP calls inside.
pub async fn capture_page(
    browser: &Browser,
    url: &str,
    base: &ClientConfigRs,
    per_call: &FetchConfigRs,
    per_shot: &ScreenshotConfigRs,
    mode: CaptureMode,
) -> Result<CaptureOutput> {
    let t0 = Instant::now();

    let timeout_ms = per_call
        .timeout_ms
        .or(per_shot.timeout_ms)
        .unwrap_or(base.timeout.navigation_ms);

    let fut = async {
        let page = browser
            .new_page("about:blank")
            .await
            .map_err(BlazeError::from)?;

        // --- Apply per-page config BEFORE navigate ---
        let (viewport_w, viewport_h) = per_shot
            .viewport
            .unwrap_or((base.viewport.width, base.viewport.height));
        page.execute(
            SetDeviceMetricsOverrideParams::builder()
                .width(viewport_w as i64)
                .height(viewport_h as i64)
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

        // Merge headers: base < per_call < per_shot (rightmost wins).
        let mut headers_map = base.network.extra_headers.clone();
        for (k, v) in &per_call.extra_headers {
            headers_map.insert(k.clone(), v.clone());
        }
        for (k, v) in &per_shot.extra_headers {
            headers_map.insert(k.clone(), v.clone());
        }
        if !headers_map.is_empty() {
            let headers = chromiumoxide::cdp::browser_protocol::network::Headers::new(
                serde_json::to_value(&headers_map).map_err(|e| BlazeError::Internal(e.to_string()))?,
            );
            page.execute(SetExtraHttpHeadersParams::new(headers)).await?;
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
                    .download_throughput(
                        base.network.download_bps.map(|x| x as f64).unwrap_or(-1.0),
                    )
                    .upload_throughput(
                        base.network.upload_bps.map(|x| x as f64).unwrap_or(-1.0),
                    )
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
            page.execute(SetScriptExecutionDisabledParams::new(true)).await?;
        }

        // --- Wire up console / error listeners BEFORE navigate ---
        let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        {
            use chromiumoxide::cdp::browser_protocol::log::EventEntryAdded;
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
        }

        // --- Navigate ---
        page.goto(url).await?;
        page.wait_for_navigation().await?;

        let final_url = page.url().await.ok().flatten().unwrap_or_else(|| url.to_string());
        // Chromium's CDP doesn't directly expose "final HTTP status" on the page. We'd
        // need Network.responseReceived listeners for that. For v1.0, we default to 200
        // on a successful navigation and 0 otherwise. Richer status capture is a v1.1 task.
        let status_code: u16 = 200;

        let mut out = CaptureOutput {
            html: None,
            png: None,
            errors: Vec::new(),
            final_url,
            status_code,
            elapsed_s: 0.0,
        };

        if matches!(mode, CaptureMode::Html | CaptureMode::Both) {
            out.html = Some(page.content().await?);
        }

        if matches!(mode, CaptureMode::Png | CaptureMode::Both) {
            let bytes = page
                .screenshot(
                    CaptureScreenshotParams::builder()
                        .format(CaptureScreenshotFormat::Png)
                        .capture_beyond_viewport(per_shot.full_page)
                        .build(),
                )
                .await?;
            out.png = Some(bytes);
        }

        out.errors = std::mem::take(&mut *errors.lock());

        // Fire-and-forget close
        let _ = page.close().await;

        Ok::<_, BlazeError>(out)
    };

    let mut result = tokio::time::timeout(Duration::from_millis(timeout_ms), fut)
        .await
        .map_err(|_| BlazeError::NavigationTimeout(timeout_ms))??;

    result.elapsed_s = (t0.elapsed().as_secs_f64() * 10000.0).round() / 10000.0;
    Ok(result)
}
