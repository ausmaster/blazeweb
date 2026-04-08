/// Unified HTTP fetch pipeline (Servo `http_loader.rs` + Chromium fetch metadata).
///
/// All resource fetching flows through a 5-function pipeline:
///
/// ```text
/// fetch(request, context)                  ← set Accept/UA/Sec-Fetch-* per destination
///   → main_fetch(request, context)         ← scheme dispatch (http/https)
///     → http_fetch(request, context)       ← cookie inject, response cookie store
///       → http_network_or_cache_fetch()    ← cache lookup, conditional headers, cache store
///         → network_fetch(request)         ← raw reqwest CLIENT (no auto-redirect)
///       → http_redirect_fetch()            ← max 20, method conversion, header stripping
/// ```

use std::sync::Arc;
use std::time::{Duration, Instant};

use wreq::header::{self, HeaderMap, HeaderName, HeaderValue};
use url::Url;
use wreq::{Client, Emulation, Method};
use wreq::http2::{Http2Options, PseudoId, PseudoOrder};
use wreq::tls::{AlpnProtocol, TlsOptions, TlsVersion};
use tokio::runtime::Runtime;
use tokio::task::JoinSet;

use std::sync::LazyLock;

use crate::error::EngineError;
use crate::net::request::{
    CacheMode, RedirectMode, Request, REQUEST_BODY_HEADERS, MAX_REDIRECTS, USER_AGENT,
};
use crate::net::response::{Response, ResponseType};

// ── Shared global runtime and client ─────────────────────────────────────────

pub(crate) static RT: LazyLock<Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("failed to create tokio runtime")
});

/// Global reqwest client with **no automatic redirect following**.
///
/// We handle redirects ourselves in `http_redirect_fetch()` so we can:
/// - Enforce the Fetch spec's 20-redirect limit
/// - Strip/modify headers per hop (Authorization, Sec-Fetch-*, etc.)
/// - Inject cookies per hop (Phase 5)
/// - Convert methods on 301/302/303
/// Build a Chrome-like TLS + HTTP/2 emulation profile.
/// Matches Chrome 131's TLS ClientHello fingerprint so WAFs
/// (Cloudflare, Akamai, AWS WAF) treat us as a real browser.
fn chrome_emulation() -> Emulation {
    // TLS config matching Chrome 131
    let tls = TlsOptions::builder()
        .enable_ocsp_stapling(true)
        .curves_list("X25519:P-256:P-384")
        .cipher_list(concat!(
            "TLS_AES_128_GCM_SHA256:",
            "TLS_AES_256_GCM_SHA384:",
            "TLS_CHACHA20_POLY1305_SHA256:",
            "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256:",
            "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256:",
            "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384:",
            "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384:",
            "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256:",
            "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256",
        ))
        .sigalgs_list(concat!(
            "ecdsa_secp256r1_sha256:",
            "rsa_pss_rsae_sha256:",
            "rsa_pkcs1_sha256:",
            "ecdsa_secp384r1_sha384:",
            "rsa_pss_rsae_sha384:",
            "rsa_pkcs1_sha384:",
            "rsa_pss_rsae_sha512:",
            "rsa_pkcs1_sha512:",
            "rsa_pkcs1_sha1",
        ))
        .alpn_protocols([AlpnProtocol::HTTP2, AlpnProtocol::HTTP1])
        .min_tls_version(TlsVersion::TLS_1_2)
        .max_tls_version(TlsVersion::TLS_1_3)
        .build();

    // HTTP/2 settings matching Chrome
    let http2 = Http2Options::builder()
        .initial_stream_id(3)
        .initial_window_size(6291456)
        .initial_connection_window_size(15728640)
        .headers_pseudo_order(
            PseudoOrder::builder()
                .extend([
                    PseudoId::Method,
                    PseudoId::Authority,
                    PseudoId::Scheme,
                    PseudoId::Path,
                ])
                .build(),
        )
        .build();

    Emulation::builder()
        .tls_options(tls)
        .http2_options(http2)
        .build()
}

pub(crate) static CLIENT: LazyLock<Client> = LazyLock::new(|| {
    let timeout_secs: u64 = std::env::var("BLAZEWEB_NETWORK_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    log::info!("HTTP client network timeout: {}s", timeout_secs);

    let connect_timeout_secs: u64 = std::env::var("BLAZEWEB_CONNECT_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);

    Client::builder()
        .emulation(chrome_emulation())
        .timeout(Duration::from_secs(timeout_secs))
        .connect_timeout(Duration::from_secs(connect_timeout_secs))
        .pool_max_idle_per_host(10)
        .redirect(wreq::redirect::Policy::none())
        .build()
        .expect("failed to create HTTP client")
});

// ── FetchContext ──────────────────────────────────────────────────────────────

/// Shared context for a fetch operation (passed through the pipeline).
///
/// Holds the cookie jar and HTTP cache. Uses `Arc<Mutex<...>>` so the
/// context can be cheaply cloned across tasks (parallel script fetch)
/// and shared between the engine, JS fetch() API, and XHR.
#[derive(Clone)]
pub struct FetchContext {
    pub base_url: Option<String>,
    pub cookie_jar: Option<Arc<std::sync::Mutex<crate::net::cookies::CookieJar>>>,
    pub http_cache: Option<Arc<std::sync::Mutex<crate::net::http_cache::HttpCache>>>,
    /// Whether to read from the HTTP cache (default: true).
    pub cache_read: bool,
    /// Whether to write to the HTTP cache (default: true).
    pub cache_write: bool,
}

impl FetchContext {
    /// Create a context with no cookie jar or cache (stateless fetch).
    pub fn new(base_url: Option<&str>) -> Self {
        Self {
            base_url: base_url.map(|s| s.to_string()),
            cookie_jar: None,
            http_cache: None,
            cache_read: true,
            cache_write: true,
        }
    }

    /// Create a context with cookies and cache enabled (new instances).
    pub fn with_cookies_and_cache(base_url: Option<&str>) -> Self {
        Self {
            base_url: base_url.map(|s| s.to_string()),
            cookie_jar: Some(Arc::new(std::sync::Mutex::new(crate::net::cookies::CookieJar::new()))),
            http_cache: Some(Arc::new(std::sync::Mutex::new(crate::net::http_cache::HttpCache::new()))),
            cache_read: true,
            cache_write: true,
        }
    }

    /// Create a context with shared (external) cache and cookie jar.
    ///
    /// Used by `Client` to pass persistent cache/cookies through the pipeline.
    pub fn with_shared(
        base_url: Option<&str>,
        cookie_jar: Arc<std::sync::Mutex<crate::net::cookies::CookieJar>>,
        http_cache: Arc<std::sync::Mutex<crate::net::http_cache::HttpCache>>,
        cache_read: bool,
        cache_write: bool,
    ) -> Self {
        Self {
            base_url: base_url.map(|s| s.to_string()),
            cookie_jar: Some(cookie_jar),
            http_cache: Some(http_cache),
            cache_read,
            cache_write,
        }
    }
}

// ── URL resolution ───────────────────────────────────────────────────────────

/// Resolve a possibly-relative `src` attribute against a base URL.
pub fn resolve_url(src: &str, base_url: Option<&str>) -> Result<Url, EngineError> {
    if let Ok(url) = Url::parse(src) {
        return Ok(url);
    }
    let base = base_url.ok_or_else(|| EngineError::Network {
        url: src.into(),
        reason: "relative script src with no base_url".into(),
    })?;
    let base_parsed = Url::parse(base).map_err(|e| EngineError::Network {
        url: base.into(),
        reason: format!("invalid base_url: {e}"),
    })?;
    base_parsed.join(src).map_err(|e| EngineError::Network {
        url: src.into(),
        reason: format!("failed to resolve URL: {e}"),
    })
}

// ── Unified fetch pipeline ───────────────────────────────────────────────────

/// Entry point: execute a request through the unified pipeline (blocking).
pub fn fetch(request: &mut Request, context: &FetchContext) -> Response {
    RT.block_on(fetch_async(request, context))
}

/// Async entry point for the unified pipeline.
pub async fn fetch_async(request: &mut Request, context: &FetchContext) -> Response {
    let t0 = Instant::now();
    let url_str = request.current_url().as_str().to_owned();
    let dest = request.destination;
    let method = request.method.clone();

    log::info!(
        "[fetch] {} {} dest={:?} mode={:?}",
        method, url_str, dest, request.mode,
    );

    set_default_headers(request);
    log::trace!(
        "[fetch] {} headers set: accept={:?}, sec-fetch-dest={:?}",
        url_str,
        request.headers.get("accept").map(|v| v.to_str().unwrap_or("?")),
        request.headers.get("sec-fetch-dest").map(|v| v.to_str().unwrap_or("?")),
    );

    let response = main_fetch(request, context).await;

    let final_url = request.current_url().as_str();
    if request.was_redirected() {
        log::info!(
            "[fetch] {} → {} ({} redirect(s)), status={}, body={} bytes, {:?}",
            url_str, final_url, request.redirect_count,
            response.status, response.body.len(), t0.elapsed(),
        );
    } else {
        log::info!(
            "[fetch] {} status={}, body={} bytes, {:?}",
            url_str, response.status, response.body.len(), t0.elapsed(),
        );
    }

    if response.is_network_error() {
        log::warn!("[fetch] {} network error: {}", url_str, response.status_text);
    }

    response
}

/// Fetch multiple requests in parallel (blocking). Returns `(index, Response)` pairs.
pub fn fetch_parallel(requests: Vec<(usize, Request)>, context: &FetchContext) -> Vec<(usize, Response)> {
    if requests.is_empty() {
        return vec![];
    }
    let count = requests.len();
    log::info!("[fetch] starting {} parallel requests", count);
    let t0 = Instant::now();
    let results = RT.block_on(fetch_parallel_async(requests, context));
    let ok_count = results.iter().filter(|(_, r)| r.ok()).count();
    let err_count = results.iter().filter(|(_, r)| r.is_network_error() || !r.ok()).count();
    log::info!(
        "[fetch] {} parallel requests complete: {} ok, {} failed, {:?}",
        count, ok_count, err_count, t0.elapsed(),
    );
    results
}

async fn fetch_parallel_async(requests: Vec<(usize, Request)>, context: &FetchContext) -> Vec<(usize, Response)> {
    let mut set = JoinSet::new();

    // Cap concurrent requests to avoid holding dozens of timing-out connections.
    // Completed requests release the permit immediately, letting queued ones start.
    let max_concurrent: usize = std::env::var("BLAZEWEB_MAX_CONCURRENT_FETCHES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrent));

    for (idx, mut request) in requests {
        let ctx = context.clone();
        let sem = semaphore.clone();
        set.spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let t0 = Instant::now();
            let url_str = request.current_url().as_str().to_owned();
            set_default_headers(&mut request);
            let response = main_fetch(&mut request, &ctx).await;
            log::debug!(
                "[fetch:{}] {} status={}, {} bytes, {:?}",
                idx, url_str, response.status, response.body.len(), t0.elapsed(),
            );
            (idx, response)
        });
    }

    let mut results = Vec::new();
    while let Some(res) = set.join_next().await {
        match res {
            Ok(pair) => results.push(pair),
            Err(e) => {
                log::error!("[fetch] parallel task panicked: {e}");
            }
        }
    }
    results
}

/// Set default Accept, User-Agent, Accept-Language, Accept-Encoding, and
/// Sec-Fetch-* headers based on the request's Destination.
fn set_default_headers(request: &mut Request) {
    // Gather values we need from request before borrowing headers mutably
    let accept = request.destination.default_accept();
    let sec_dest = request.destination.sec_fetch_dest();
    let sec_mode = request.mode.sec_fetch_mode();
    let is_nav = request.destination.is_navigation();
    let user_act = request.user_activation;
    let cache_mode = request.cache_mode;

    let is_trustworthy = {
        let url = request.current_url();
        url.scheme() == "https" || is_localhost(url)
    };

    let h = &mut request.headers;

    // Accept — only set if not already present (SetHeaderIfMissing pattern)
    if !h.contains_key("accept") {
        h.insert("accept", HeaderValue::from_static(accept));
    }

    // User-Agent
    if !h.contains_key("user-agent") {
        if let Ok(val) = HeaderValue::from_str(USER_AGENT) {
            h.insert("user-agent", val);
        }
    }

    // Accept-Language
    if !h.contains_key("accept-language") {
        h.insert("accept-language", HeaderValue::from_static("en-US,en;q=0.9"));
    }

    // Accept-Encoding
    if !h.contains_key("accept-encoding") {
        h.insert("accept-encoding", HeaderValue::from_static("gzip, deflate, br"));
    }

    // Sec-CH-UA Client Hints — Chrome sends these on every request by default.
    // Missing these is a strong bot signal for modern WAFs (Cloudflare, Akamai, AWS WAF).
    if !h.contains_key("sec-ch-ua") {
        h.insert("sec-ch-ua", HeaderValue::from_static(
            "\"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\", \"Google Chrome\";v=\"131\""
        ));
    }
    if !h.contains_key("sec-ch-ua-mobile") {
        h.insert("sec-ch-ua-mobile", HeaderValue::from_static("?0"));
    }
    if !h.contains_key("sec-ch-ua-platform") {
        h.insert("sec-ch-ua-platform", HeaderValue::from_static("\"Linux\""));
    }

    // Sec-Fetch-* headers — only on trustworthy URLs (https, localhost)
    if is_trustworthy {
        if let Ok(val) = HeaderValue::from_str(sec_dest) {
            h.insert("sec-fetch-dest", val);
        }
        if let Ok(val) = HeaderValue::from_str(sec_mode) {
            h.insert("sec-fetch-mode", val);
        }
        h.insert("sec-fetch-site", HeaderValue::from_static("none"));
        if user_act && is_nav {
            h.insert("sec-fetch-user", HeaderValue::from_static("?1"));
        }
    }

    // Upgrade-Insecure-Requests for navigations
    if is_nav {
        h.insert("upgrade-insecure-requests", HeaderValue::from_static("1"));
    }

    // Cache-mode specific headers
    match cache_mode {
        CacheMode::NoCache => {
            if !h.contains_key("cache-control") {
                h.insert("cache-control", HeaderValue::from_static("max-age=0"));
            }
        }
        CacheMode::Reload | CacheMode::NoStore => {
            if !h.contains_key("pragma") {
                h.insert("pragma", HeaderValue::from_static("no-cache"));
            }
            if !h.contains_key("cache-control") {
                h.insert("cache-control", HeaderValue::from_static("no-cache"));
            }
        }
        _ => {}
    }
}

fn is_localhost(url: &Url) -> bool {
    url.host_str()
        .is_some_and(|h| h == "localhost" || h == "127.0.0.1" || h == "::1")
}

/// Scheme dispatch: http/https → http_fetch, data: → data_fetch.
async fn main_fetch(request: &mut Request, context: &FetchContext) -> Response {
    let scheme = request.current_url().scheme().to_string();
    match scheme.as_str() {
        "http" | "https" => http_fetch(request, context).await,
        "data" => data_fetch(request),
        _ => {
            log::warn!("[fetch] unsupported scheme: {}", scheme);
            Response::network_error(&format!("unsupported scheme: {}", scheme))
        }
    }
}

/// Fetch a `data:` URL per Fetch spec §6.
///
/// Parses the data URL, decodes the body (percent-encoded or base64),
/// and returns a synthetic response with the decoded body and MIME type.
fn data_fetch(request: &Request) -> Response {
    let url = request.current_url();
    let url_str = url.as_str();

    match data_url::DataUrl::process(url_str) {
        Ok(data_url) => {
            match data_url.decode_to_vec() {
                Ok((body, _fragment)) => {
                    let mime = data_url.mime_type();
                    let content_type = mime.to_string();
                    log::debug!(
                        "[fetch:data] decoded {} bytes, content-type: {}",
                        body.len(), content_type,
                    );
                    let mut headers = wreq::header::HeaderMap::new();
                    if let Ok(val) = wreq::header::HeaderValue::from_str(&content_type) {
                        headers.insert(wreq::header::CONTENT_TYPE, val);
                    }
                    Response {
                        response_type: super::response::ResponseType::Basic,
                        status: 200,
                        status_text: "OK".to_string(),
                        headers,
                        body,
                        url_list: vec![url.clone()],
                    }
                }
                Err(e) => {
                    log::warn!("[fetch:data] decode failed: {:?}", e);
                    Response::network_error(&format!("data URL decode failed: {:?}", e))
                }
            }
        }
        Err(e) => {
            log::warn!("[fetch:data] parse failed: {:?}", e);
            Response::network_error(&format!("data URL parse failed: {:?}", e))
        }
    }
}

/// HTTP-level fetch: network request + redirect handling.
///
/// Injects cookies from the jar before sending, stores Set-Cookie from the
/// response. Cookie operations are guarded by the FetchContext's mutex.
async fn http_fetch(request: &mut Request, context: &FetchContext) -> Response {
    // Inject cookies into request
    if let Some(jar_mutex) = &context.cookie_jar {
        if let Ok(mut jar) = jar_mutex.lock() {
            let url = request.current_url().clone();
            crate::net::cookies::set_request_cookies(
                &url, &mut request.headers, &mut jar,
            );
        }
    }

    let response = http_network_or_cache_fetch(request, context).await;

    // Store response cookies
    if let Some(jar_mutex) = &context.cookie_jar {
        if let Ok(mut jar) = jar_mutex.lock() {
            crate::net::cookies::set_cookies_from_headers(
                request.current_url(), &response.headers, &mut jar,
            );
        }
    }

    if response.is_redirect() {
        let location = response.headers.get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("?");
        log::debug!(
            "[fetch] {} {} → redirect {} to {}",
            request.method, request.current_url(), response.status, location,
        );

        match request.redirect_mode {
            RedirectMode::Error => {
                log::warn!(
                    "[fetch] {} redirect blocked (mode=error)",
                    request.current_url(),
                );
                return Response::network_error("redirect not allowed (redirect mode: error)");
            }
            RedirectMode::Manual => {
                log::debug!(
                    "[fetch] {} redirect returned as opaque (mode=manual)",
                    request.current_url(),
                );
                return Response {
                    response_type: ResponseType::Opaque,
                    ..response
                };
            }
            RedirectMode::Follow => {
                return http_redirect_fetch(request, response, context).await;
            }
        }
    }

    response
}

/// Network-or-cache fetch: check cache, issue conditional requests, store responses.
///
/// RFC 7234 §4: Constructing Responses from Caches.
/// Respects `context.cache_read` and `context.cache_write` flags.
async fn http_network_or_cache_fetch(request: &mut Request, context: &FetchContext) -> Response {
    // Cache lookup (only if cache_read is enabled)
    if context.cache_read {
        if let Some(cache_mutex) = &context.http_cache {
            if let Ok(cache) = cache_mutex.lock() {
                // Try to serve from cache
                if let Some(cached_response) = cache.construct_response(request) {
                    return cached_response;
                }

                // Add conditional headers for revalidation
                if let Some(reval_headers) = cache.get_revalidation_headers(request) {
                    for (name, value) in reval_headers.iter() {
                        request.headers.insert(name.clone(), value.clone());
                    }
                }
            }
        }
    }

    let response = network_fetch(request).await;

    // Handle 304 Not Modified → refresh cached entry (needs cache_read to have sent conditional headers)
    if response.status == 304 {
        if let Some(cache_mutex) = &context.http_cache {
            if let Ok(mut cache) = cache_mutex.lock() {
                if let Some(refreshed) = cache.refresh(request, &response) {
                    return refreshed;
                }
            }
        }
    }

    // Store cacheable responses (only if cache_write is enabled)
    if context.cache_write {
        if response.ok() || crate::net::http_cache::is_default_cacheable(response.status) {
            if let Some(cache_mutex) = &context.http_cache {
                if let Ok(mut cache) = cache_mutex.lock() {
                    cache.store(request, &response);
                }
            }
        }
    }

    // Invalidate cache on unsafe methods with success
    if !request.method.is_safe() && (200..400).contains(&response.status) {
        if let Some(cache_mutex) = &context.http_cache {
            if let Ok(mut cache) = cache_mutex.lock() {
                cache.invalidate(request.current_url());
            }
        }
    }

    response
}

/// Raw network fetch via the global reqwest CLIENT.
async fn network_fetch(request: &Request) -> Response {
    let url = request.current_url().clone();
    let url_str = url.as_str().to_owned();
    let t0 = Instant::now();

    log::debug!("[fetch:net] {} {}", request.method, url_str);

    let mut req = CLIENT.request(request.method.clone(), url.as_str());

    for (name, value) in request.headers.iter() {
        req = req.header(name, value);
    }

    if let Some(body) = &request.body {
        req = req.body(body.clone());
        log::trace!("[fetch:net] {} body={} bytes", url_str, body.len());
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            log::warn!("[fetch:net] {} send error: {}", url_str, e);
            return Response::network_error(&format!("{}: {}", url_str, e));
        }
    };

    let status = resp.status().as_u16();
    let status_text = resp.status().canonical_reason().unwrap_or("").to_string();
    let headers = resp.headers().clone();
    let url_list = request.url_list.clone();

    let body = match resp.bytes().await {
        Ok(b) => b.to_vec(),
        Err(e) => {
            log::warn!("[fetch:net] {} body read error: {}", url_str, e);
            return Response::network_error(&format!("{}: body read error: {}", url_str, e));
        }
    };

    log::debug!(
        "[fetch:net] {} → {} {} ({} bytes, {:?})",
        url_str, status, status_text, body.len(), t0.elapsed(),
    );

    Response {
        response_type: ResponseType::Basic,
        status,
        status_text,
        headers,
        body,
        url_list,
    }
}

/// HTTP redirect fetch (Fetch spec §4.4).
///
/// Uses `Box::pin` for recursion through `main_fetch`.
async fn http_redirect_fetch(
    request: &mut Request,
    response: Response,
    context: &FetchContext,
) -> Response {
    // Parse Location header
    let location_url = match response.location_url(request.current_url()) {
        Some(url) => url,
        None => {
            log::debug!(
                "[fetch:redirect] {} status {} but no valid Location header",
                request.current_url(), response.status,
            );
            return response;
        }
    };

    // Only http/https redirect targets
    let scheme = location_url.scheme();
    if scheme != "http" && scheme != "https" {
        log::warn!(
            "[fetch:redirect] {} → {} unsupported scheme",
            request.current_url(), location_url,
        );
        return Response::network_error(&format!("redirect to unsupported scheme: {}", scheme));
    }

    // Redirect count limit
    if request.redirect_count >= MAX_REDIRECTS {
        log::warn!(
            "[fetch:redirect] {} exceeded {} redirects",
            request.current_url(), MAX_REDIRECTS,
        );
        return Response::network_error("too many redirects (max 20)");
    }
    request.redirect_count += 1;

    // Method conversion (301/302 + POST → GET, 303 + non-GET/HEAD → GET)
    let old_method = request.method.clone();
    let status = response.status;
    if (status == 301 || status == 302) && request.method == Method::POST {
        request.method = Method::GET;
        request.body = None;
        strip_body_headers(&mut request.headers);
        log::debug!(
            "[fetch:redirect] {} {} → GET (status {})",
            request.redirect_count, old_method, status,
        );
    } else if status == 303 && request.method != Method::GET && request.method != Method::HEAD {
        request.method = Method::GET;
        request.body = None;
        strip_body_headers(&mut request.headers);
        log::debug!(
            "[fetch:redirect] {} {} → GET (status 303)",
            request.redirect_count, old_method,
        );
    }

    // Cross-origin redirect: strip Authorization, nullify Origin
    let is_cross_origin = !same_origin(request.current_url(), &location_url);
    if is_cross_origin {
        let stripped_auth = request.headers.remove("authorization").is_some();
        if request.headers.contains_key("origin") {
            request.headers.insert("origin", HeaderValue::from_static("null"));
        }
        log::debug!(
            "[fetch:redirect] {} cross-origin → {}{}",
            request.redirect_count, location_url,
            if stripped_auth { " (stripped Authorization)" } else { "" },
        );
    } else {
        log::debug!(
            "[fetch:redirect] {} same-origin → {}",
            request.redirect_count, location_url,
        );
    }

    // Append redirect target to url_list
    request.url_list.push(location_url);

    // Re-compute Sec-Fetch-* headers for the new URL
    recompute_sec_fetch_headers(request);

    // Recurse via Box::pin to avoid infinite future size
    Box::pin(main_fetch(request, context)).await
}

/// Strip and re-add Sec-Fetch-* headers for the current URL.
fn recompute_sec_fetch_headers(request: &mut Request) {
    let sec_keys: Vec<HeaderName> = request
        .headers
        .keys()
        .filter(|k| k.as_str().starts_with("sec-fetch-") || k.as_str().starts_with("sec-ch-"))
        .cloned()
        .collect();
    for key in sec_keys {
        request.headers.remove(&key);
    }

    let is_trustworthy = {
        let url = request.current_url();
        url.scheme() == "https" || is_localhost(url)
    };

    if is_trustworthy {
        if let Ok(val) = HeaderValue::from_str(request.destination.sec_fetch_dest()) {
            request.headers.insert("sec-fetch-dest", val);
        }
        if let Ok(val) = HeaderValue::from_str(request.mode.sec_fetch_mode()) {
            request.headers.insert("sec-fetch-mode", val);
        }
        request.headers.insert("sec-fetch-site", HeaderValue::from_static("cross-site"));
    }
}

fn strip_body_headers(headers: &mut HeaderMap) {
    for name in REQUEST_BODY_HEADERS {
        if let Ok(header_name) = name.parse::<HeaderName>() {
            headers.remove(header_name);
        }
    }
}

fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme() && a.host() == b.host() && a.port_or_known_default() == b.port_or_known_default()
}

#[cfg(test)]
#[path = "fetch_tests.rs"]
mod tests;

