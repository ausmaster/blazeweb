//! Core navigate-and-capture step, running on a page drawn from the pool.
//!
//! Driven by `Client` via tokio. All async, all inside `py.allow_threads()`.
//! One pooled page per fetch: configure (per-call overrides only), navigate,
//! capture, reset on error. Pool pages keep their base config + console
//! listeners across fetches.

use std::time::{Duration, Instant};

use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
use chromiumoxide::cdp::browser_protocol::fetch::{
    DisableParams as FetchDisableParams, EnableParams as FetchEnableParams, EventRequestPaused,
    FailRequestParams, RequestPattern,
};
use chromiumoxide::cdp::browser_protocol::network::{
    ErrorReason, ResourceType, SetBlockedUrLsParams, SetExtraHttpHeadersParams,
};
use chromiumoxide::cdp::browser_protocol::page::{
    AddScriptToEvaluateOnNewDocumentParams, CaptureScreenshotFormat, CaptureScreenshotParams,
    EventDomContentEventFired, EventLoadEventFired, EventNavigatedWithinDocument, NavigateParams,
    ReferrerPolicy, RemoveScriptToEvaluateOnNewDocumentParams, ScriptIdentifier,
};
use futures::StreamExt;

use crate::config::{
    ActionErrorPolicy, ActionRs, ClientConfigRs, FetchConfigRs, ImageFormat, ScreenshotConfigRs,
    WaitUntil,
};
use crate::error::{BlazeError, Result};
use crate::pool::{PageGuard, block_patterns};
use crate::result::ConsoleMessageRs;

/// True when ``target`` differs from ``prev`` only by URL fragment (the part
/// after `#`) AND the fragment actually differs. chromium treats such
/// transitions as same-document navigations: no new HTTP request, no `load`
/// event, no `domContentLoaded` event — only `Page.navigatedWithinDocument`
/// fires.
///
/// Identical URLs (same path/query, same fragment or both fragmentless) are
/// NOT same-doc — chromium does a full reload, the init scripts re-fire, and
/// the load event fires; we want the normal goto path.
///
/// Used by `capture_page` to route hash-only navs through `Runtime.evaluate`
/// (which goes through a separate CDP command channel) rather than
/// `Page.navigate` (which empirically hangs in chromiumoxide for hash-only
/// URLs after a previous nav on the same pool tab).
fn is_same_document_change(prev: &str, target: &str) -> bool {
    fn split(s: &str) -> (&str, Option<&str>) {
        match s.split_once('#') {
            Some((p, h)) => (p, Some(h)),
            None => (s, None),
        }
    }
    let (prev_prefix, prev_hash) = split(prev);
    let (target_prefix, target_hash) = split(target);
    prev_prefix == target_prefix && prev_hash != target_hash
}

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
    /// JSON-string per ``FetchConfig.post_load_scripts`` entry, ``None`` for
    /// ``undefined`` / non-serializable JS returns. Empty when no
    /// ``post_load_scripts`` were configured.
    pub post_load_results: Vec<Option<String>>,
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

        // Per-call header merge — only if there ARE per-call / per-shot extras
        // OR a base Referer that needs lifting out.
        let mut headers_map = base.network.extra_headers.clone();
        for (k, v) in &per_call.extra_headers {
            headers_map.insert(k.clone(), v.clone());
        }
        for (k, v) in &per_shot.extra_headers {
            headers_map.insert(k.clone(), v.clone());
        }
        // Lift `Referer` (case-insensitive) out of the merged headers map.
        // Setting Referer via Network.setExtraHTTPHeaders is rejected by
        // chromium with ERR_BLOCKED_BY_CLIENT for cross-origin values (W3C
        // Referrer Policy enforcement at the URL loader). The supported
        // path is `Page.navigate` with the `referrer` parameter, applied
        // below.
        let mut referrer: Option<String> = None;
        let referer_keys: Vec<String> = headers_map
            .keys()
            .filter(|k| k.eq_ignore_ascii_case("referer"))
            .cloned()
            .collect();
        for k in referer_keys {
            referrer = headers_map.remove(&k);
        }
        let has_per_fetch_extras =
            !per_call.extra_headers.is_empty() || !per_shot.extra_headers.is_empty();
        if has_per_fetch_extras || referrer.is_some() {
            log::trace!(
                target: "blazeweb::engine",
                "[{url}] merging headers (per_call={}, per_shot={}, referrer={})",
                per_call.extra_headers.len(),
                per_shot.extra_headers.len(),
                referrer.is_some()
            );
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
        // Same-document navs (hash-only / pushState) fire neither DCL nor
        // load; they fire `Page.navigatedWithinDocument` instead. Subscribe
        // here so we don't hang on a hash-only fetch from a previously-loaded
        // pool tab.
        //
        // Empirically, on chrome-headless-shell, `Page.navigate` for a URL
        // that differs only by hash from the pool tab's current URL never
        // returns through chromiumoxide's CommandFuture path — likely the
        // chromiumoxide handler's pending-navigations queue blocks waiting
        // for a `load` event that chromium will never fire for same-doc
        // navs. We race `goto` against `navigatedWithinDocument`: if the
        // event fires before goto returns, the navigation is same-doc and
        // already complete; we drop the pending goto future and skip the
        // lifecycle wait.
        let mut within_doc_stream = page
            .event_listener::<EventNavigatedWithinDocument>()
            .await
            .map_err(BlazeError::from)?;

        // Detect same-document navigation by URL comparison against the pool
        // tab's tracked current URL. Same-doc navs need a different code path
        // because chromiumoxide's `Page.navigate` command future hangs for
        // hash-only URLs in some sequences (e.g., hash → hash); the symptom
        // is no response from chromium and no `navigatedWithinDocument` event
        // firing on the pool tab's per-fetch subscription.
        //
        // `Runtime.evaluate` goes through a separate CDP command channel and
        // doesn't share that hang. Setting `location.href = url` triggers the
        // same-document nav natively, which fires `navigatedWithinDocument`
        // reliably.
        let same_doc_nav = match guard.current_url() {
            Some(prev) if is_same_document_change(&prev, url) => {
                log::trace!(
                    target: "blazeweb::engine",
                    "[{url}] same-doc nav detected (prev={prev}); using Runtime.evaluate"
                );
                let escaped = serde_json::to_string(url).unwrap_or_else(|_| "''".to_string());
                page.evaluate(format!("location.href = {escaped};").as_str())
                    .await?;
                let t_nav_ack = t_goto.elapsed();
                log::trace!(target: "blazeweb::engine", "[{url}] evaluate-nav ack in {t_nav_ack:?}");
                true
            }
            _ => {
                log::trace!(target: "blazeweb::engine", "[{url}] navigate (wait_until={wait_until:?})");
                let nav_params = match referrer.as_ref() {
                    // ReferrerPolicy::UnsafeUrl tells chromium to pass the
                    // full referrer URL through unchanged. The default policy
                    // (`strict-origin-when-cross-origin`) strips path and
                    // query for cross-origin requests, which would mangle
                    // testing-tool use cases that need the exact Referer
                    // they specified.
                    Some(r) => NavigateParams::builder()
                        .url(url.to_string())
                        .referrer(r.clone())
                        .referrer_policy(ReferrerPolicy::UnsafeUrl)
                        .build()
                        .map_err(|e| BlazeError::Cdp(format!("navigate params: {e}")))?,
                    None => NavigateParams::new(url.to_string()),
                };
                page.goto(nav_params).await?;
                let t_nav_ack = t_goto.elapsed();
                log::trace!(target: "blazeweb::engine", "[{url}] navigate ack in {t_nav_ack:?}");
                false
            }
        };

        if !same_doc_nav {
            match wait_until {
                WaitUntil::DomContentLoaded => {
                    // DCL preferred; load covers tiny docs that never fire DCL;
                    // navigatedWithinDocument covers same-doc navs that race
                    // goto's response (rare but possible on full nav too).
                    tokio::select! {
                        _ = dcl_stream.next() => {
                            log::trace!(target: "blazeweb::engine", "[{url}] DCL fired");
                        }
                        _ = load_stream.next() => {
                            log::trace!(target: "blazeweb::engine", "[{url}] load fired (no DCL)");
                        }
                        _ = within_doc_stream.next() => {
                            log::trace!(target: "blazeweb::engine", "[{url}] navigatedWithinDocument fired");
                        }
                    }
                }
                WaitUntil::Load => {
                    tokio::select! {
                        _ = load_stream.next() => {
                            log::trace!(target: "blazeweb::engine", "[{url}] load fired");
                        }
                        _ = within_doc_stream.next() => {
                            log::trace!(target: "blazeweb::engine", "[{url}] navigatedWithinDocument fired");
                        }
                    }
                }
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

        // Per-call navigation blocking — arm AFTER the initial load + settle
        // so the original page is reachable, but BEFORE actions so any
        // JS-driven navigation they trigger is intercepted. Filter at the
        // Fetch.enable layer (resource_type=Document) so subresources never
        // fire ``Fetch.requestPaused`` — they go through chromium's normal
        // path with zero CDP overhead. Cleanup (``Fetch.disable``) runs
        // unconditionally below.
        if per_call.block_navigation {
            log::trace!(
                target: "blazeweb::engine",
                "[{url}] block_navigation: enabling Fetch interception (Document only)"
            );
            let document_only = RequestPattern::builder()
                .resource_type(ResourceType::Document)
                .build();
            page.execute(FetchEnableParams::builder().pattern(document_only).build())
                .await?;
            let mut paused_stream = page
                .event_listener::<EventRequestPaused>()
                .await
                .map_err(BlazeError::from)?;
            let page_for_task = page.clone();
            tokio::spawn(async move {
                while let Some(evt) = paused_stream.next().await {
                    // Pattern guarantees only Document-type requests reach
                    // us; abort each.
                    let _ = page_for_task
                        .execute(FailRequestParams::new(
                            evt.request_id.clone(),
                            ErrorReason::Aborted,
                        ))
                        .await;
                }
            });
        }

        // Per-call post-load scripts — run arbitrary JS on the fully-loaded
        // page via Runtime.evaluate. Single CDP roundtrip per script. The
        // primary primitive for "do JS work on the loaded page" use cases
        // (see CLAUDE.md "Public Python surface").
        //
        // Capture each script's JS return value. chromiumoxide forces
        // `return_by_value=true` on the underlying CDP `Runtime.evaluate`,
        // so primitives + plain objects + arrays come back JSON-decoded
        // inside `RemoteObject.value`.
        //
        // Function returns: chromiumoxide preserves
        // `RemoteObject.type=Function` even with returnByValue=true; we
        // surface those as `None` rather than the empty dict chromium
        // would otherwise produce.
        //
        // Sharp edge: DOM nodes / Window serialize to `{}` under
        // returnByValue=true (chromium can't enumerate them). Consumers
        // who need to distinguish should filter in their own script —
        // e.g. ``"JSON.stringify(x) === '{}' ? null : x"``. Wrapping all
        // scripts in a JS trampoline that detects DOM nodes up front
        // would break multi-statement / `throw` / `try-catch` sources
        // that aren't valid in an `await (EXPR)` slot, so the engine
        // intentionally does not.
        let mut post_load_results: Vec<Option<String>> =
            Vec::with_capacity(per_call.post_load_scripts.len());
        for (i, src) in per_call.post_load_scripts.iter().enumerate() {
            log::trace!(
                target: "blazeweb::engine",
                "[{url}] post_load_script[{i}] ({} chars)",
                src.len()
            );
            let eval_result = page.evaluate(src.as_str()).await?;
            let is_function = matches!(
                eval_result.object().r#type,
                chromiumoxide::cdp::js_protocol::runtime::RemoteObjectType::Function
            );
            let serialized = if is_function {
                None
            } else {
                eval_result.value().map(|v| v.to_string())
            };
            if serialized.is_none() {
                log::debug!(
                    target: "blazeweb::engine",
                    "[{url}] post_load_script[{i}] returned non-serializable / undefined / function"
                );
            }
            post_load_results.push(serialized);
        }

        // Optional settle window AFTER post_load_scripts and BEFORE actions /
        // capture. Distinct from `wait_after_ms` (which fires before
        // post_load_scripts). Lets scripts that schedule async work
        // (setTimeout, fetch, deferred DOM mutations) finish before capture.
        let wait_after_post_load_ms = per_call
            .wait_after_post_load_ms
            .or(per_shot.wait_after_post_load_ms)
            .unwrap_or(base.wait_after_post_load_ms);
        if wait_after_post_load_ms > 0 {
            log::trace!(
                target: "blazeweb::engine",
                "[{url}] post-load settle {wait_after_post_load_ms}ms"
            );
            tokio::time::sleep(Duration::from_millis(wait_after_post_load_ms)).await;
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
        // on response headers, independent of wait_until choice. Same-document
        // navs don't trigger a new HTTP response, so fall back to the prior
        // fetch's status (the document hasn't actually changed).
        let status_code: u16 = guard
            .main_status()
            .or_else(|| same_doc_nav.then(|| guard.prev_main_status()).flatten())
            .unwrap_or(0);

        let mut out = CaptureOutput {
            html: None,
            png: None,
            console_messages: Vec::new(),
            final_url,
            status_code,
            elapsed_s: 0.0,
            post_load_results,
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

    // Per-call extra-headers cleanup — restore the Client-level baseline so
    // per-call overrides don't leak to subsequent fetches on this pooled tab.
    // Mirrors the block_urls cleanup pattern; same swallow-error semantics
    // (cleanup failure must not mask the original cause). Triggers on either
    // per_call/per_shot extras OR a base Referer that we lifted out — both
    // paths called `setExtraHTTPHeaders` with a modified header set.
    if (!per_call.extra_headers.is_empty() || !per_shot.extra_headers.is_empty())
        && let Ok(json) = serde_json::to_value(&base.network.extra_headers)
    {
        let headers = chromiumoxide::cdp::browser_protocol::network::Headers::new(json);
        let _ = page.execute(SetExtraHttpHeadersParams::new(headers)).await;
    }

    // Per-call navigation-block cleanup — disable the Fetch domain so the
    // listener task's stream ends and any future fetches on this tab aren't
    // intercepted. CDP auto-continues paused requests on disable.
    if per_call.block_navigation {
        let _ = page.execute(FetchDisableParams::default()).await;
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
