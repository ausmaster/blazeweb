//! Core navigate-and-capture step, running on a page drawn from the pool.
//!
//! Driven by `Client` via tokio. All async, all inside `py.allow_threads()`.
//! One pooled page per fetch: configure (per-call overrides only), navigate,
//! capture, reset on error. Pool pages keep their base config + console
//! listeners across fetches.

use std::time::{Duration, Instant};

use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
use chromiumoxide::cdp::browser_protocol::network::{
    SetBlockedUrLsParams, SetExtraHttpHeadersParams,
};
use chromiumoxide::cdp::browser_protocol::page::{
    AddScriptToEvaluateOnNewDocumentParams, CaptureScreenshotFormat, CaptureScreenshotParams,
    EventDomContentEventFired, EventLoadEventFired, RemoveScriptToEvaluateOnNewDocumentParams,
    ScriptIdentifier,
};
use futures::StreamExt;

use crate::config::{
    ActionErrorPolicy, ActionRs, ClientConfigRs, FetchConfigRs, ImageFormat, ScreenshotConfigRs,
    WaitUntil,
};
use crate::error::{BlazeError, Result};
use crate::pool::{PageGuard, block_patterns};
use crate::result::ConsoleMessageRs;

/// Apply an action's failure policy. Returns ``Ok(true)`` if the action
/// succeeded (caller should run any post-action wait), ``Ok(false)`` if it
/// failed but the policy said to continue/ignore, or ``Err`` to propagate
/// (policy=Abort).
fn handle_action_result(
    res: Result<()>,
    policy: ActionErrorPolicy,
    guard: &PageGuard,
    action_name: &str,
    selector: &str,
) -> Result<bool> {
    match res {
        Ok(()) => Ok(true),
        Err(e) => match policy {
            ActionErrorPolicy::Abort => Err(e),
            ActionErrorPolicy::Continue => {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0);
                guard.console_messages().lock().push(ConsoleMessageRs {
                    kind: "error".to_string(),
                    text: format!("Action {action_name}({selector}) failed: {e}"),
                    timestamp,
                });
                Ok(false)
            }
            ActionErrorPolicy::Ignore => {
                log::debug!(
                    target: "blazeweb::engine",
                    "action {action_name}({selector}) ignored error: {e}"
                );
                Ok(false)
            }
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureMode {
    Html,
    Png,
    Both,
}

pub struct CaptureOutput {
    pub html: Option<String>,
    pub png: Option<Vec<u8>>,
    pub console_messages: Vec<ConsoleMessageRs>,
    pub final_url: String,
    pub status_code: u16,
    pub elapsed_s: f64,
}

/// Navigate an already-configured pooled page to `url` and capture.
pub async fn capture_page(
    guard: &PageGuard,
    url: &str,
    base: &ClientConfigRs,
    per_call: &FetchConfigRs,
    per_shot: &ScreenshotConfigRs,
    mode: CaptureMode,
) -> Result<CaptureOutput> {
    let t0 = Instant::now();
    log::debug!(target: "blazeweb::engine", "[{url}] capture_page mode={mode:?}");

    let timeout_ms = per_call
        .timeout_ms
        .or(per_shot.timeout_ms)
        .unwrap_or(base.timeout.navigation_ms);

    let page = guard.page();

    // Per-call init scripts — register BEFORE the timeout-wrapped main work
    // so we hold their identifiers in outer scope for cleanup. ``page.execute``
    // for ``Page.addScriptToEvaluateOnNewDocument`` is fast (one CDP RTT) and
    // shouldn't itself block long enough to need the lifecycle timeout.
    // Cleanup runs unconditionally below — success or failure path.
    let mut script_ids: Vec<ScriptIdentifier> = Vec::with_capacity(per_call.scripts.len());
    for src in &per_call.scripts {
        log::trace!(
            target: "blazeweb::engine",
            "[{url}] registering per-call init script ({} chars)",
            src.len()
        );
        let resp = page
            .execute(AddScriptToEvaluateOnNewDocumentParams::new(src.clone()))
            .await?;
        script_ids.push(resp.identifier.clone());
    }

    let fut = async {
        // Per-call viewport override (e.g. different size just for this screenshot).
        if let Some((w, h)) = per_shot.viewport {
            log::trace!(target: "blazeweb::engine", "[{url}] override viewport {w}x{h}");
            page.execute(
                SetDeviceMetricsOverrideParams::builder()
                    .width(w as i64)
                    .height(h as i64)
                    .device_scale_factor(base.viewport.device_scale_factor)
                    .mobile(base.viewport.mobile)
                    .build()
                    .map_err(|e| BlazeError::Cdp(format!("metrics: {e}")))?,
            )
            .await?;
        }

        // Per-call URL blocking — apply merged ``base + per-call`` list via
        // Network.setBlockedURLs. Cycle 4 will pair this with restoration to
        // the base list so per-call entries don't leak across fetches.
        if !per_call.block_urls.is_empty() {
            let mut merged = base.network.block_urls.clone();
            merged.extend(per_call.block_urls.iter().cloned());
            log::trace!(
                target: "blazeweb::engine",
                "[{url}] applying {} blocked URLs (base={}, per_call={})",
                merged.len(),
                base.network.block_urls.len(),
                per_call.block_urls.len()
            );
            page.execute(SetBlockedUrLsParams {
                url_patterns: Some(block_patterns(&merged)),
            })
            .await?;
        }

        // Per-call header merge — only if there ARE per-call / per-shot extras.
        if !per_call.extra_headers.is_empty() || !per_shot.extra_headers.is_empty() {
            log::trace!(
                target: "blazeweb::engine",
                "[{url}] merging headers (per_call={}, per_shot={})",
                per_call.extra_headers.len(),
                per_shot.extra_headers.len()
            );
            let mut headers_map = base.network.extra_headers.clone();
            for (k, v) in &per_call.extra_headers {
                headers_map.insert(k.clone(), v.clone());
            }
            for (k, v) in &per_shot.extra_headers {
                headers_map.insert(k.clone(), v.clone());
            }
            let headers = chromiumoxide::cdp::browser_protocol::network::Headers::new(
                serde_json::to_value(&headers_map)
                    .map_err(|e| BlazeError::Internal(e.to_string()))?,
            );
            page.execute(SetExtraHttpHeadersParams::new(headers))
                .await?;
        }

        // Subscribe BEFORE goto (race-free). goto returns on navigate ack
        // (~5-10ms), well before any lifecycle event — DO NOT race it against
        // these streams or the goto arm always wins.
        let wait_until = per_call
            .wait_until
            .or(per_shot.wait_until)
            .unwrap_or(base.wait_until);
        let t_goto = Instant::now();
        log::trace!(target: "blazeweb::engine", "[{url}] subscribe lifecycle streams");
        let mut dcl_stream = page
            .event_listener::<EventDomContentEventFired>()
            .await
            .map_err(BlazeError::from)?;
        let mut load_stream = page
            .event_listener::<EventLoadEventFired>()
            .await
            .map_err(BlazeError::from)?;

        log::trace!(target: "blazeweb::engine", "[{url}] navigate (wait_until={wait_until:?})");
        page.goto(url).await?;
        let t_nav_ack = t_goto.elapsed();
        log::trace!(target: "blazeweb::engine", "[{url}] navigate ack in {t_nav_ack:?}");

        match wait_until {
            WaitUntil::DomContentLoaded => {
                // DCL preferred; load covers tiny docs that never fire DCL.
                tokio::select! {
                    _ = dcl_stream.next() => {
                        log::trace!(target: "blazeweb::engine", "[{url}] DCL fired");
                    }
                    _ = load_stream.next() => {
                        log::trace!(target: "blazeweb::engine", "[{url}] load fired (no DCL)");
                    }
                }
            }
            WaitUntil::Load => {
                load_stream.next().await;
                log::trace!(target: "blazeweb::engine", "[{url}] load fired");
            }
        }
        log::trace!(
            target: "blazeweb::engine",
            "[{url}] nav done in {:?}",
            t_goto.elapsed()
        );

        // Optional post-event settle — lets late async JS mutate the DOM on
        // SPAs that render AFTER the chosen lifecycle event fires.
        let wait_after_ms = per_call
            .wait_after_ms
            .or(per_shot.wait_after_ms)
            .unwrap_or(base.wait_after_ms);
        if wait_after_ms > 0 {
            log::trace!(target: "blazeweb::engine", "[{url}] settle {wait_after_ms}ms");
            tokio::time::sleep(Duration::from_millis(wait_after_ms)).await;
        }

        // Run post-load actions BEFORE HTML capture so the captured DOM
        // reflects post-action state (and a Click that triggers nav still
        // gets a final_url update from the response listener).
        for action in &per_call.actions {
            match action {
                ActionRs::Click {
                    selector,
                    wait_after_ms: w,
                    on_error,
                } => {
                    log::trace!(target: "blazeweb::engine", "[{url}] click action: {selector}");
                    let res = async {
                        let element = page.find_element(selector).await?;
                        element.click().await?;
                        Ok::<_, BlazeError>(())
                    }
                    .await;
                    let ok = handle_action_result(res, *on_error, guard, "click", selector)?;
                    if ok && *w > 0 {
                        tokio::time::sleep(Duration::from_millis(*w)).await;
                    }
                }
                ActionRs::Fill {
                    selector,
                    value,
                    wait_after_ms: w,
                    on_error,
                } => {
                    log::trace!(target: "blazeweb::engine", "[{url}] fill action: {selector}");
                    let res = async {
                        let element = page.find_element(selector).await?;
                        let value_js = serde_json::to_string(value)
                            .map_err(|e| BlazeError::Internal(format!("fill value: {e}")))?;
                        let fn_src = format!(
                            "function() {{ \
                                this.focus(); \
                                this.value = {value_js}; \
                                this.dispatchEvent(new Event('input', {{bubbles: true}})); \
                                this.dispatchEvent(new Event('change', {{bubbles: true}})); \
                            }}"
                        );
                        element.call_js_fn(fn_src, false).await?;
                        Ok::<_, BlazeError>(())
                    }
                    .await;
                    let ok = handle_action_result(res, *on_error, guard, "fill", selector)?;
                    if ok && *w > 0 {
                        tokio::time::sleep(Duration::from_millis(*w)).await;
                    }
                }
                ActionRs::Hover {
                    selector,
                    wait_after_ms: w,
                    on_error,
                } => {
                    log::trace!(target: "blazeweb::engine", "[{url}] hover action: {selector}");
                    let res = async {
                        let element = page.find_element(selector).await?;
                        element.hover().await?;
                        Ok::<_, BlazeError>(())
                    }
                    .await;
                    let ok = handle_action_result(res, *on_error, guard, "hover", selector)?;
                    if ok && *w > 0 {
                        tokio::time::sleep(Duration::from_millis(*w)).await;
                    }
                }
                ActionRs::Wait { duration_ms } => {
                    log::trace!(target: "blazeweb::engine", "[{url}] wait action: {duration_ms}ms");
                    tokio::time::sleep(Duration::from_millis(*duration_ms)).await;
                }
            }
        }

        let final_url = page
            .url()
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| url.to_string());
        // Status from the pool's Network.responseReceived listener — captured
        // on response headers, independent of wait_until choice.
        let status_code: u16 = guard.main_status().unwrap_or(0);

        let mut out = CaptureOutput {
            html: None,
            png: None,
            console_messages: Vec::new(),
            final_url,
            status_code,
            elapsed_s: 0.0,
        };

        if matches!(mode, CaptureMode::Html | CaptureMode::Both) {
            let t_html = Instant::now();
            let html = page.content().await?;
            log::trace!(
                target: "blazeweb::engine",
                "[{url}] content: {} bytes in {:?}",
                html.len(),
                t_html.elapsed()
            );
            out.html = Some(html);
        }

        if matches!(mode, CaptureMode::Png | CaptureMode::Both) {
            let cdp_format = match per_shot.format {
                ImageFormat::Png => CaptureScreenshotFormat::Png,
                ImageFormat::Jpeg => CaptureScreenshotFormat::Jpeg,
                ImageFormat::Webp => CaptureScreenshotFormat::Webp,
            };
            let mut builder = CaptureScreenshotParams::builder()
                .format(cdp_format)
                .capture_beyond_viewport(per_shot.full_page);
            if let Some(q) = per_shot.quality {
                builder = builder.quality(q as i64);
            }
            let t_shot = Instant::now();
            let bytes = page.screenshot(builder.build()).await?;
            log::trace!(
                target: "blazeweb::engine",
                "[{url}] screenshot: {} bytes ({:?}, format={:?})",
                bytes.len(),
                t_shot.elapsed(),
                per_shot.format
            );
            out.png = Some(bytes);
        }

        // Drain accumulated console messages for this fetch.
        out.console_messages = std::mem::take(&mut *guard.console_messages().lock());
        if !out.console_messages.is_empty() {
            log::trace!(
                target: "blazeweb::engine",
                "[{url}] drained {} console messages",
                out.console_messages.len()
            );
        }

        Ok::<_, BlazeError>(out)
    };

    let fut_result = tokio::time::timeout(Duration::from_millis(timeout_ms), fut).await;

    // Per-call init script cleanup — runs unconditionally so we never leak
    // scripts to the next fetch on this pooled tab. Errors here are swallowed:
    // the page may already be in a bad state if we got here via a CDP failure,
    // and surfacing a cleanup error would mask the original cause.
    for id in &script_ids {
        let _ = page
            .execute(RemoveScriptToEvaluateOnNewDocumentParams::new(id.clone()))
            .await;
    }

    // Per-call URL-block cleanup — restore the Client-level baseline so the
    // per-call additions don't leak to the next fetch. Sending an empty
    // pattern list clears all blocks if the base list is itself empty.
    if !per_call.block_urls.is_empty() {
        let _ = page
            .execute(SetBlockedUrLsParams {
                url_patterns: Some(block_patterns(&base.network.block_urls)),
            })
            .await;
    }

    // On error, reset the page so the next URL on this tab isn't poisoned by
    // a half-loaded predecessor.
    if matches!(&fut_result, Err(_) | Ok(Err(_))) {
        log::debug!(target: "blazeweb::engine", "[{url}] error — reset to about:blank");
        let _ = tokio::time::timeout(Duration::from_secs(2), async {
            let _ = page.goto("about:blank").await;
        })
        .await;
    }

    let mut result = fut_result.map_err(|_| {
        log::warn!(target: "blazeweb::engine", "[{url}] nav timeout after {timeout_ms}ms");
        BlazeError::NavigationTimeout(timeout_ms)
    })??;

    result.elapsed_s = (t0.elapsed().as_secs_f64() * 10000.0).round() / 10000.0;
    log::debug!(
        target: "blazeweb::engine",
        "[{url}] complete in {:.3}s (status={}, console_messages={})",
        result.elapsed_s,
        result.status_code,
        result.console_messages.len()
    );
    Ok(result)
}
