/// RFC 7234/7232 HTTP cache (modeled on Servo's `http_cache.rs`).
///
/// Provides cache storage, freshness calculation, conditional request headers,
/// and 304 Not Modified handling. Used by `http_network_or_cache_fetch()` to
/// avoid redundant network requests.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};

use wreq::header::HeaderMap;
#[cfg(test)]
use wreq::header::HeaderValue;
use wreq::Method;
use url::Url;

use crate::net::request::Request;
use crate::net::response::{Response, ResponseType};

/// A cached HTTP response with metadata for freshness and revalidation.
#[derive(Debug, Clone)]
#[allow(dead_code)] // RFC 7234 fields — not all used yet
struct CachedResource {
    /// Original request headers (for Vary matching).
    request_headers: HeaderMap,
    /// Cached response status.
    status: u16,
    /// Cached response status text.
    status_text: String,
    /// Cached response headers.
    headers: HeaderMap,
    /// Cached response body.
    body: Vec<u8>,
    /// Full URL list (for redirect chain reconstruction).
    url_list: Vec<Url>,
    /// When this entry was stored.
    stored_at: Instant,
    /// Computed freshness lifetime.
    freshness_lifetime: Duration,
    /// When this entry was last validated with the server.
    last_validated: Instant,
}

impl CachedResource {
    /// Whether this cached resource is still fresh.
    fn is_fresh(&self) -> bool {
        self.last_validated.elapsed() < self.freshness_lifetime
    }

    /// Build a Response from this cached resource.
    fn to_response(&self) -> Response {
        Response {
            response_type: ResponseType::Basic,
            status: self.status,
            status_text: self.status_text.clone(),
            headers: self.headers.clone(),
            body: self.body.clone(),
            url_list: self.url_list.clone(),
        }
    }
}

/// RFC 7234 HTTP response cache.
///
/// Keyed by URL. Only caches GET responses. Supports:
/// - Cache-Control directives (max-age, no-cache, no-store, must-revalidate)
/// - ETag / If-None-Match conditional requests
/// - Last-Modified / If-Modified-Since conditional requests
/// - 304 Not Modified → reuse cached body, merge headers
/// - Cache invalidation on unsafe methods (POST/PUT/DELETE)
pub struct HttpCache {
    entries: HashMap<String, Vec<CachedResource>>,
}

impl HttpCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Look up a cached response for a request.
    ///
    /// Returns `Some(response)` if a fresh cached entry exists, `None` otherwise.
    /// Only serves GET requests from cache.
    pub fn construct_response(&self, request: &Request) -> Option<Response> {
        if request.method != Method::GET {
            return None;
        }

        let url_key = request.current_url().as_str();
        let entries = self.entries.get(url_key)?;

        for entry in entries {
            // Vary matching: check that request headers match
            if !self.vary_matches(request, entry) {
                continue;
            }

            if entry.is_fresh() {
                log::debug!(
                    "[cache] HIT {} (fresh, {}s remaining)",
                    url_key,
                    entry.freshness_lifetime.saturating_sub(entry.last_validated.elapsed()).as_secs(),
                );
                return Some(entry.to_response());
            } else {
                log::debug!("[cache] STALE {} (needs revalidation)", url_key);
                return None; // Stale — caller should revalidate
            }
        }

        log::debug!("[cache] MISS {}", url_key);
        None
    }

    /// Get conditional request headers (If-None-Match, If-Modified-Since)
    /// for revalidation of a stale cached entry.
    pub fn get_revalidation_headers(&self, request: &Request) -> Option<HeaderMap> {
        if request.method != Method::GET {
            return None;
        }

        let url_key = request.current_url().as_str();
        let entries = self.entries.get(url_key)?;

        for entry in entries {
            if !self.vary_matches(request, entry) {
                continue;
            }

            let mut headers = HeaderMap::new();

            // ETag → If-None-Match
            if let Some(etag) = entry.headers.get("etag") {
                headers.insert("if-none-match", etag.clone());
            }

            // Last-Modified → If-Modified-Since
            if let Some(lm) = entry.headers.get("last-modified") {
                headers.insert("if-modified-since", lm.clone());
            }

            if !headers.is_empty() {
                log::debug!(
                    "[cache] revalidation headers for {}: etag={}, lm={}",
                    url_key,
                    headers.contains_key("if-none-match"),
                    headers.contains_key("if-modified-since"),
                );
                return Some(headers);
            }
        }

        None
    }

    /// Handle a 304 Not Modified response: merge new headers, reuse cached body.
    ///
    /// RFC 7234 §4.3.4.
    pub fn refresh(&mut self, request: &Request, response_304: &Response) -> Option<Response> {
        if response_304.status != 304 {
            return None;
        }
        if request.method != Method::GET {
            return None;
        }

        let url_key = request.current_url().as_str();
        let entries = self.entries.get_mut(url_key)?;

        for entry in entries.iter_mut() {
            if !Self::vary_matches_static(request, entry) {
                continue;
            }

            // Merge headers from 304 response into cached entry
            for (name, value) in response_304.headers.iter() {
                entry.headers.insert(name.clone(), value.clone());
            }

            // Reset freshness
            entry.freshness_lifetime = compute_freshness_lifetime(&entry.headers);
            entry.last_validated = Instant::now();

            log::debug!(
                "[cache] refreshed {} (new freshness: {}s)",
                url_key, entry.freshness_lifetime.as_secs(),
            );

            return Some(entry.to_response());
        }

        None
    }

    /// Store a response in the cache.
    ///
    /// Only caches GET responses that are cacheable per RFC 7234.
    pub fn store(&mut self, request: &Request, response: &Response) {
        if request.method != Method::GET {
            return;
        }

        if !response_is_cacheable(response) {
            log::trace!("[cache] not cacheable: {}", request.current_url());
            return;
        }

        // Don't cache responses with Authorization header (shared cache rule)
        if request.headers.contains_key("authorization") {
            if !has_cache_control_directive(&response.headers, "public") {
                log::trace!("[cache] skip caching (Authorization present, no public directive)");
                return;
            }
        }

        let freshness_lifetime = compute_freshness_lifetime(&response.headers);
        let now = Instant::now();

        let entry = CachedResource {
            request_headers: request.headers.clone(),
            status: response.status,
            status_text: response.status_text.clone(),
            headers: response.headers.clone(),
            body: response.body.clone(),
            url_list: response.url_list.clone(),
            stored_at: now,
            freshness_lifetime,
            last_validated: now,
        };

        let url_key = request.current_url().as_str().to_owned();
        log::debug!(
            "[cache] stored {} ({} bytes, freshness={}s)",
            url_key, response.body.len(), freshness_lifetime.as_secs(),
        );

        // Replace any existing entry for this URL
        self.entries.insert(url_key, vec![entry]);
    }

    /// Invalidate cache entries for a URL (after unsafe method like POST).
    ///
    /// RFC 7234 §4.4.
    pub fn invalidate(&mut self, url: &Url) {
        let url_key = url.as_str();
        if self.entries.remove(url_key).is_some() {
            log::debug!("[cache] invalidated {}", url_key);
        }
    }

    /// Total number of cached entries.
    pub fn len(&self) -> usize {
        self.entries.values().map(|v| v.len()).sum()
    }

    /// Whether the cache is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all cached entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Check Vary header matching between a request and cached entry.
    fn vary_matches(&self, request: &Request, entry: &CachedResource) -> bool {
        Self::vary_matches_static(request, entry)
    }

    fn vary_matches_static(request: &Request, entry: &CachedResource) -> bool {
        let Some(vary) = entry.headers.get("vary") else {
            return true; // No Vary header — always matches
        };
        let vary_str = vary.to_str().unwrap_or("");
        if vary_str == "*" {
            return false; // Vary: * never matches
        }
        for header_name in vary_str.split(',') {
            let header_name = header_name.trim().to_lowercase();
            let current = request.headers.get(&header_name).map(|v| v.as_bytes());
            let cached = entry.request_headers.get(&header_name).map(|v| v.as_bytes());
            if current != cached {
                return false;
            }
        }
        true
    }
}

// ── Cache-Control parsing helpers ────────────────────────────────────────────

/// Whether a response is cacheable (RFC 7234 §3).
fn response_is_cacheable(response: &Response) -> bool {
    // no-store → not cacheable
    if has_cache_control_directive(&response.headers, "no-store") {
        return false;
    }

    // Pragma: no-cache (only when no Cache-Control)
    if !response.headers.contains_key("cache-control") {
        if let Some(pragma) = response.headers.get("pragma") {
            if pragma.to_str().unwrap_or("").contains("no-cache") {
                return false;
            }
        }
    }

    // Cacheable if: has explicit cache directives or heuristic info
    if has_cache_control_directive(&response.headers, "public")
        || has_cache_control_directive(&response.headers, "max-age")
        || has_cache_control_directive(&response.headers, "s-maxage")
        || has_cache_control_directive(&response.headers, "no-cache")
        || response.headers.contains_key("expires")
        || response.headers.contains_key("etag")
        || response.headers.contains_key("last-modified")
    {
        return true;
    }

    // Cacheable by default status codes
    is_cacheable_by_default(response.status)
}

/// Status codes cacheable by default (RFC 7231 §6.1).
pub fn is_default_cacheable(status: u16) -> bool {
    matches!(status, 200 | 203 | 204 | 206 | 300 | 301 | 404 | 405 | 410 | 414 | 501)
}

/// Alias used internally.
fn is_cacheable_by_default(status: u16) -> bool {
    is_default_cacheable(status)
}

/// Compute freshness lifetime from response headers (RFC 7234 §4.2.2).
fn compute_freshness_lifetime(headers: &HeaderMap) -> Duration {
    // no-cache → must revalidate every time
    if has_cache_control_directive(headers, "no-cache") {
        return Duration::ZERO;
    }

    // max-age directive
    if let Some(max_age) = get_cache_control_max_age(headers) {
        let age = get_age_header(headers);
        return max_age.saturating_sub(age);
    }

    // Expires header
    if let Some(expires_str) = headers.get("expires").and_then(|v| v.to_str().ok()) {
        if let Ok(expires) = httpdate::parse_http_date(expires_str) {
            if let Ok(remaining) = expires.duration_since(SystemTime::now()) {
                return remaining;
            }
            return Duration::ZERO; // Already expired
        }
    }

    // Heuristic: (now - last_modified) / 10, capped at 24h
    if let Some(lm_str) = headers.get("last-modified").and_then(|v| v.to_str().ok()) {
        if let Ok(lm) = httpdate::parse_http_date(lm_str) {
            if let Ok(age) = SystemTime::now().duration_since(lm) {
                let heuristic = age / 10;
                let max_24h = Duration::from_secs(24 * 3600);
                let capped = heuristic.min(max_24h);
                let current_age = get_age_header(headers);
                return capped.saturating_sub(current_age);
            }
        }
    }

    // Default heuristic: 5 minutes for responses with no explicit freshness info.
    // Browsers cache 200 responses even without cache headers — this matches
    // practical browser behavior for static resources like scripts and stylesheets.
    Duration::from_secs(300)
}

/// Check if a Cache-Control directive is present.
fn has_cache_control_directive(headers: &HeaderMap, directive: &str) -> bool {
    headers.get("cache-control")
        .and_then(|v| v.to_str().ok())
        .map(|cc| cc.split(',').any(|d| d.trim().to_lowercase().starts_with(directive)))
        .unwrap_or(false)
}

/// Extract max-age value from Cache-Control.
fn get_cache_control_max_age(headers: &HeaderMap) -> Option<Duration> {
    let cc = headers.get("cache-control")?.to_str().ok()?;
    for directive in cc.split(',') {
        let d = directive.trim().to_lowercase();
        if d.starts_with("max-age=") || d.starts_with("s-maxage=") {
            if let Some(val) = d.split('=').nth(1) {
                if let Ok(secs) = val.trim().parse::<u64>() {
                    return Some(Duration::from_secs(secs));
                }
            }
        }
    }
    None
}

/// Get Age header value.
fn get_age_header(headers: &HeaderMap) -> Duration {
    headers.get("age")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::ZERO)
}

#[cfg(test)]
#[path = "http_cache_tests.rs"]
mod tests;

