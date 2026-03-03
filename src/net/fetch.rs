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

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method, Url};
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
pub(crate) static CLIENT: LazyLock<Client> = LazyLock::new(|| {
    let mut root_store = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };

    let native_result = rustls_native_certs::load_native_certs();
    for cert in native_result.certs {
        let _ = root_store.add(cert);
    }

    let tls_config = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("valid TLS protocol versions")
        .with_root_certificates(root_store)
        .with_no_client_auth();

    Client::builder()
        .timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(10)
        .redirect(reqwest::redirect::Policy::none())
        .use_preconfigured_tls(tls_config)
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

    for (idx, mut request) in requests {
        // Clone the shared context (cheap — Arc clones)
        let ctx = context.clone();
        set.spawn(async move {
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

/// Scheme dispatch: http/https → http_fetch.
async fn main_fetch(request: &mut Request, context: &FetchContext) -> Response {
    let scheme = request.current_url().scheme().to_string();
    match scheme.as_str() {
        "http" | "https" => http_fetch(request, context).await,
        _ => {
            log::warn!("[fetch] unsupported scheme: {}", scheme);
            Response::network_error(&format!("unsupported scheme: {}", scheme))
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

    let mut req = CLIENT.request(request.method.clone(), url);

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
mod tests {
    use super::*;

    #[test]
    fn test_resolve_absolute_url() {
        let url = resolve_url("https://example.com/app.js", None).unwrap();
        assert_eq!(url.as_str(), "https://example.com/app.js");
    }

    #[test]
    fn test_resolve_relative_url() {
        let url = resolve_url("lib.js", Some("https://example.com/page/")).unwrap();
        assert_eq!(url.as_str(), "https://example.com/page/lib.js");
    }

    #[test]
    fn test_resolve_absolute_path() {
        let url = resolve_url("/scripts/app.js", Some("https://example.com/page/")).unwrap();
        assert_eq!(url.as_str(), "https://example.com/scripts/app.js");
    }

    #[test]
    fn test_resolve_relative_no_base() {
        let err = resolve_url("app.js", None).unwrap_err();
        assert!(err.to_string().contains("no base_url"));
    }

    #[test]
    fn test_client_initializes() {
        let _ = CLIENT.get("https://example.com");
    }

    #[test]
    fn test_set_default_headers_document() {
        let url = Url::parse("https://example.com").unwrap();
        let mut req = Request::document(url);
        set_default_headers(&mut req);
        assert!(req.headers.get("accept").unwrap().to_str().unwrap().starts_with("text/html"));
        assert!(req.headers.get("user-agent").unwrap().to_str().unwrap().contains("Blazeweb"));
        assert_eq!(req.headers.get("sec-fetch-dest").unwrap(), "document");
        assert_eq!(req.headers.get("sec-fetch-mode").unwrap(), "navigate");
        assert_eq!(req.headers.get("sec-fetch-site").unwrap(), "none");
        assert_eq!(req.headers.get("sec-fetch-user").unwrap(), "?1");
        assert_eq!(req.headers.get("upgrade-insecure-requests").unwrap(), "1");
    }

    #[test]
    fn test_set_default_headers_script() {
        let url = Url::parse("https://cdn.example.com/app.js").unwrap();
        let mut req = Request::script(url);
        set_default_headers(&mut req);
        assert_eq!(req.headers.get("accept").unwrap(), "*/*");
        assert_eq!(req.headers.get("sec-fetch-dest").unwrap(), "script");
        assert_eq!(req.headers.get("sec-fetch-mode").unwrap(), "no-cors");
        assert!(req.headers.get("sec-fetch-user").is_none());
        assert!(req.headers.get("upgrade-insecure-requests").is_none());
    }

    #[test]
    fn test_set_default_headers_no_sec_on_http() {
        let url = Url::parse("http://example.com/page").unwrap();
        let mut req = Request::document(url);
        set_default_headers(&mut req);
        assert!(req.headers.get("sec-fetch-dest").is_none());
        assert!(req.headers.get("sec-fetch-mode").is_none());
    }

    #[test]
    fn test_set_default_headers_preserves_existing() {
        let url = Url::parse("https://api.example.com/data").unwrap();
        let mut req = Request::fetch_api(url, Method::GET);
        req.headers.insert("accept", HeaderValue::from_static("application/json"));
        set_default_headers(&mut req);
        assert_eq!(req.headers.get("accept").unwrap(), "application/json");
    }

    #[test]
    fn test_same_origin() {
        let a = Url::parse("https://example.com/a").unwrap();
        let b = Url::parse("https://example.com/b").unwrap();
        assert!(same_origin(&a, &b));

        let c = Url::parse("https://other.com/a").unwrap();
        assert!(!same_origin(&a, &c));

        let d = Url::parse("http://example.com/a").unwrap();
        assert!(!same_origin(&a, &d));
    }

    #[test]
    fn test_strip_body_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("text/plain"));
        headers.insert("content-length", HeaderValue::from_static("42"));
        headers.insert("authorization", HeaderValue::from_static("Bearer token"));
        headers.insert("accept", HeaderValue::from_static("*/*"));
        strip_body_headers(&mut headers);
        assert!(headers.get("content-type").is_none());
        assert!(headers.get("content-length").is_none());
        assert!(headers.get("authorization").is_some());
        assert!(headers.get("accept").is_some());
    }

    #[test]
    fn test_is_localhost() {
        assert!(is_localhost(&Url::parse("http://localhost:8080").unwrap()));
        assert!(is_localhost(&Url::parse("http://127.0.0.1").unwrap()));
        assert!(!is_localhost(&Url::parse("http://example.com").unwrap()));
    }

    #[test]
    fn test_fetch_document_http() {
        let url = Url::parse("http://example.com").unwrap();
        let ctx = FetchContext::new(Some("http://example.com"));
        let mut req = Request::document(url);
        let resp = fetch(&mut req, &ctx);
        assert!(resp.ok());
        assert!(resp.text().contains("Example Domain"));
    }

    #[test]
    fn test_fetch_document_https() {
        let url = Url::parse("https://httpbin.org/html").unwrap();
        let ctx = FetchContext::new(Some("https://httpbin.org/html"));
        let mut req = Request::document(url);
        let resp = fetch(&mut req, &ctx);
        assert!(resp.ok());
        assert!(resp.text().contains("Herman Melville"));
    }

    #[test]
    fn test_fetch_redirect() {
        // httpbin.org/redirect/1 does one redirect to /get
        let url = Url::parse("https://httpbin.org/redirect/1").unwrap();
        let ctx = FetchContext::new(None);
        let mut req = Request::document(url);
        let resp = fetch(&mut req, &ctx);
        assert!(resp.ok());
        assert!(resp.was_redirected());
    }
}
