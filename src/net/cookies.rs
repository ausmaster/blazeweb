/// RFC 6265 cookie jar (modeled on Servo's `cookie_storage.rs`).
///
/// Provides per-URL cookie matching with domain/path/secure/httponly/expiry.
/// Used by `http_fetch()` to inject `Cookie` headers and store `Set-Cookie`
/// responses.

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use cookie::Cookie;
use reqwest::header::HeaderMap;
use reqwest::Url;

/// Maximum cookies per registrable domain.
const MAX_COOKIES_PER_HOST: usize = 150;

/// A stored cookie with metadata beyond what the `cookie` crate tracks.
#[derive(Debug, Clone)]
struct StoredCookie {
    /// The parsed cookie.
    cookie: Cookie<'static>,
    /// When this cookie was created.
    creation_time: SystemTime,
    /// When this cookie was last accessed (for LRU eviction).
    last_access_time: SystemTime,
    /// Whether this cookie was set via HTTP (vs JS `document.cookie`).
    http_only_flag: bool,
    /// Host-only flag: if true, only exact domain match (no subdomain).
    host_only: bool,
    /// Persistent flag: has an explicit expiry.
    persistent: bool,
    /// Expiry time (only meaningful if persistent).
    expiry_time: Option<SystemTime>,
}

impl StoredCookie {
    /// Whether this cookie has expired.
    fn is_expired(&self) -> bool {
        if let Some(expiry) = self.expiry_time {
            SystemTime::now() > expiry
        } else {
            false // Session cookies don't expire
        }
    }

    /// Touch — update last access time.
    fn touch(&mut self) {
        self.last_access_time = SystemTime::now();
    }
}

/// RFC 6265 cookie jar.
///
/// Cookies are bucketed by registrable domain (or host for IP addresses).
/// Supports domain matching, path matching, Secure, HttpOnly, and expiry.
pub struct CookieJar {
    /// domain → list of cookies.
    cookies: HashMap<String, Vec<StoredCookie>>,
    max_per_host: usize,
}

impl CookieJar {
    pub fn new() -> Self {
        Self {
            cookies: HashMap::new(),
            max_per_host: MAX_COOKIES_PER_HOST,
        }
    }

    /// Generate the `Cookie` header value for a request URL.
    ///
    /// Returns `None` if no cookies match.
    /// RFC 6265 §5.4.
    pub fn cookies_for_url(&mut self, url: &Url) -> Option<String> {
        let host = url.host_str()?;
        let path = url.path();
        let is_secure = url.scheme() == "https";
        let now = SystemTime::now();

        // Collect matching cookies — we need to iterate each domain bucket separately
        // to avoid borrowing self.cookies mutably more than once.
        let domains_to_check = candidate_domains(host);
        let mut matching: Vec<(String, String, usize, SystemTime)> = Vec::new(); // (name, value, path_len, creation_time)

        for domain in &domains_to_check {
            if let Some(cookies) = self.cookies.get_mut(domain.as_str()) {
                // Remove expired cookies first
                cookies.retain(|c| !c.is_expired());

                for cookie in cookies.iter_mut() {
                    if !domain_matches(host, cookie.cookie.domain().unwrap_or(""), cookie.host_only) {
                        continue;
                    }
                    if !path_matches(path, cookie.cookie.path().unwrap_or("/")) {
                        continue;
                    }
                    if cookie.cookie.secure().unwrap_or(false) && !is_secure {
                        continue;
                    }
                    cookie.last_access_time = now;
                    matching.push((
                        cookie.cookie.name().to_string(),
                        cookie.cookie.value().to_string(),
                        cookie.cookie.path().unwrap_or("/").len(),
                        cookie.creation_time,
                    ));
                }
            }
        }

        if matching.is_empty() {
            return None;
        }

        // Sort: longer path first, then older creation time first
        matching.sort_by(|a, b| {
            b.2.cmp(&a.2)
                .then_with(|| a.3.cmp(&b.3))
        });

        let cookie_str = matching
            .iter()
            .map(|(name, value, _, _)| format!("{}={}", name, value))
            .collect::<Vec<_>>()
            .join("; ");

        log::debug!(
            "[cookies] {} matching cookies for {}",
            matching.len(), url,
        );

        Some(cookie_str)
    }

    /// Process `Set-Cookie` headers from a response.
    ///
    /// RFC 6265 §5.3.
    pub fn set_cookies_from_headers(&mut self, url: &Url, headers: &HeaderMap) {
        let Some(host) = url.host_str() else { return };
        let mut count = 0u32;

        for value in headers.get_all("set-cookie") {
            let Ok(cookie_str) = value.to_str() else { continue };
            match Cookie::parse(cookie_str.to_owned()) {
                Ok(cookie) => {
                    self.store_cookie(cookie.into_owned(), url, host);
                    count += 1;
                }
                Err(e) => {
                    log::debug!("[cookies] failed to parse Set-Cookie '{}': {}", cookie_str, e);
                }
            }
        }

        if count > 0 {
            log::debug!("[cookies] stored {} cookie(s) from {}", count, url);
        }
    }

    /// Store a single parsed cookie.
    fn store_cookie(&mut self, cookie: Cookie<'static>, url: &Url, host: &str) {
        let now = SystemTime::now();

        // Determine domain
        let (domain_key, host_only) = if let Some(domain) = cookie.domain() {
            let domain = domain.trim_start_matches('.');
            // Reject if domain doesn't domain-match the request host
            if !host.ends_with(domain) && host != domain {
                log::trace!(
                    "[cookies] rejecting cookie: domain {} doesn't match host {}",
                    domain, host,
                );
                return;
            }
            (domain.to_lowercase(), false)
        } else {
            (host.to_lowercase(), true)
        };

        // Determine path
        let path = cookie.path()
            .map(|p| p.to_string())
            .unwrap_or_else(|| default_path(url.path()));

        // Determine expiry
        let (persistent, expiry_time) = if let Some(max_age) = cookie.max_age() {
            let secs = max_age.whole_seconds().max(0) as u64;
            (true, Some(now + Duration::from_secs(secs)))
        } else if let Some(_expires) = cookie.expires() {
            // cookie crate's expires() returns an Expiration, but we'll use
            // a simplified approach: treat as session if we can't parse it
            // In practice, max-age takes precedence in RFC 6265 §5.3
            (false, None)
        } else {
            (false, None)
        };

        // Check secure-only: non-secure URL can't set Secure cookies
        if cookie.secure().unwrap_or(false) && url.scheme() != "https" {
            log::trace!(
                "[cookies] rejecting Secure cookie from non-secure URL: {}",
                url,
            );
            return;
        }

        let http_only_flag = cookie.http_only().unwrap_or(false);

        // Build final cookie with domain and path set
        let mut final_cookie = cookie;
        final_cookie.set_domain(domain_key.clone());
        final_cookie.set_path(path);

        let stored = StoredCookie {
            cookie: final_cookie,
            creation_time: now,
            last_access_time: now,
            http_only_flag,
            host_only,
            persistent,
            expiry_time,
        };

        // Remove existing cookie with same name/domain/path
        let cookies = self.cookies.entry(domain_key.clone()).or_default();
        let name = stored.cookie.name().to_string();
        let path_str = stored.cookie.path().unwrap_or("/").to_string();
        cookies.retain(|c| {
            !(c.cookie.name() == name && c.cookie.path().unwrap_or("/") == path_str)
        });

        // Evict if at capacity
        if cookies.len() >= self.max_per_host {
            // Remove expired first
            cookies.retain(|c| !c.is_expired());

            if cookies.len() >= self.max_per_host {
                // Evict oldest by last_access_time
                if let Some(oldest_idx) = cookies.iter().enumerate()
                    .min_by_key(|(_, c)| c.last_access_time)
                    .map(|(i, _)| i)
                {
                    log::trace!(
                        "[cookies] evicting oldest cookie for {}",
                        domain_key,
                    );
                    cookies.remove(oldest_idx);
                }
            }
        }

        // Don't store if already expired
        if stored.is_expired() {
            return;
        }

        cookies.push(stored);
    }

    /// Total number of cookies stored.
    pub fn len(&self) -> usize {
        self.cookies.values().map(|v| v.len()).sum()
    }

    /// Whether the jar is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear all cookies.
    pub fn clear(&mut self) {
        self.cookies.clear();
    }
}

// ── Cookie matching helpers (RFC 6265 §5.1) ─────────────────────────────────

/// RFC 6265 §5.1.3: Domain matching.
fn domain_matches(host: &str, domain: &str, host_only: bool) -> bool {
    let domain = domain.trim_start_matches('.');
    if host_only {
        host.eq_ignore_ascii_case(domain)
    } else {
        host.eq_ignore_ascii_case(domain)
            || (host.ends_with(&format!(".{}", domain))
                && host.len() > domain.len() + 1)
    }
}

/// RFC 6265 §5.1.4: Path matching.
fn path_matches(request_path: &str, cookie_path: &str) -> bool {
    if request_path == cookie_path {
        return true;
    }
    if request_path.starts_with(cookie_path) {
        if cookie_path.ends_with('/') {
            return true;
        }
        if request_path.as_bytes().get(cookie_path.len()) == Some(&b'/') {
            return true;
        }
    }
    false
}

/// RFC 6265 §5.1.4: Default path from request URI path.
fn default_path(request_path: &str) -> String {
    if !request_path.starts_with('/') {
        return "/".to_string();
    }
    if let Some(last_slash) = request_path.rfind('/') {
        if last_slash == 0 {
            return "/".to_string();
        }
        return request_path[..last_slash].to_string();
    }
    "/".to_string()
}

/// Generate candidate domain keys to check for a given host.
fn candidate_domains(host: &str) -> Vec<String> {
    let mut domains = vec![host.to_lowercase()];
    // Add parent domains for subdomain matching
    let parts: Vec<&str> = host.split('.').collect();
    for i in 1..parts.len().saturating_sub(1) {
        domains.push(parts[i..].join(".").to_lowercase());
    }
    domains
}

// ── Pipeline integration helpers ─────────────────────────────────────────────

/// Inject cookies into request headers.
///
/// Called by `http_fetch()` before sending the request.
pub fn set_request_cookies(url: &Url, headers: &mut HeaderMap, jar: &mut CookieJar) {
    if let Some(cookie_str) = jar.cookies_for_url(url) {
        if let Ok(val) = reqwest::header::HeaderValue::from_str(&cookie_str) {
            headers.insert("cookie", val);
        }
    }
}

/// Store response cookies from Set-Cookie headers.
///
/// Called by `http_fetch()` after receiving the response.
pub fn set_cookies_from_headers(url: &Url, headers: &HeaderMap, jar: &mut CookieJar) {
    jar.set_cookies_from_headers(url, headers);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_matches() {
        assert!(domain_matches("example.com", "example.com", true));
        assert!(!domain_matches("sub.example.com", "example.com", true));
        assert!(domain_matches("sub.example.com", "example.com", false));
        assert!(domain_matches("example.com", "example.com", false));
        assert!(!domain_matches("notexample.com", "example.com", false));
    }

    #[test]
    fn test_path_matches() {
        assert!(path_matches("/", "/"));
        assert!(path_matches("/foo", "/foo"));
        assert!(path_matches("/foo/bar", "/foo"));
        assert!(path_matches("/foo/bar", "/foo/"));
        assert!(!path_matches("/foobar", "/foo"));
        assert!(path_matches("/foo/bar/baz", "/foo/bar"));
    }

    #[test]
    fn test_default_path() {
        assert_eq!(default_path("/foo/bar"), "/foo");
        assert_eq!(default_path("/foo"), "/");
        assert_eq!(default_path("/"), "/");
        assert_eq!(default_path(""), "/");
        assert_eq!(default_path("/a/b/c"), "/a/b");
    }

    #[test]
    fn test_cookie_jar_basic() {
        let mut jar = CookieJar::new();
        let url = Url::parse("https://example.com/page").unwrap();

        // Set a cookie
        let mut headers = HeaderMap::new();
        headers.insert("set-cookie", "name=value; Path=/".parse().unwrap());
        jar.set_cookies_from_headers(&url, &headers);

        assert_eq!(jar.len(), 1);

        // Retrieve it
        let cookies = jar.cookies_for_url(&url);
        assert_eq!(cookies.as_deref(), Some("name=value"));
    }

    #[test]
    fn test_cookie_jar_path_scope() {
        let mut jar = CookieJar::new();
        let url = Url::parse("https://example.com/api/v1").unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("set-cookie", "token=abc; Path=/api".parse().unwrap());
        jar.set_cookies_from_headers(&url, &headers);

        // Should match /api/v1
        let cookies = jar.cookies_for_url(&Url::parse("https://example.com/api/v1").unwrap());
        assert_eq!(cookies.as_deref(), Some("token=abc"));

        // Should NOT match /other
        let cookies = jar.cookies_for_url(&Url::parse("https://example.com/other").unwrap());
        assert!(cookies.is_none());
    }

    #[test]
    fn test_cookie_jar_secure() {
        let mut jar = CookieJar::new();
        let url = Url::parse("https://example.com/").unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("set-cookie", "sec=yes; Secure; Path=/".parse().unwrap());
        jar.set_cookies_from_headers(&url, &headers);

        // Should match on https
        let cookies = jar.cookies_for_url(&Url::parse("https://example.com/").unwrap());
        assert!(cookies.is_some());

        // Should NOT match on http
        let cookies = jar.cookies_for_url(&Url::parse("http://example.com/").unwrap());
        assert!(cookies.is_none());
    }

    #[test]
    fn test_cookie_jar_domain() {
        let mut jar = CookieJar::new();
        let url = Url::parse("https://www.example.com/").unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("set-cookie", "name=val; Domain=example.com; Path=/".parse().unwrap());
        jar.set_cookies_from_headers(&url, &headers);

        // Should match sub.example.com
        let cookies = jar.cookies_for_url(&Url::parse("https://sub.example.com/").unwrap());
        assert_eq!(cookies.as_deref(), Some("name=val"));

        // Should match example.com
        let cookies = jar.cookies_for_url(&Url::parse("https://example.com/").unwrap());
        assert_eq!(cookies.as_deref(), Some("name=val"));
    }

    #[test]
    fn test_cookie_jar_replace() {
        let mut jar = CookieJar::new();
        let url = Url::parse("https://example.com/").unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("set-cookie", "name=old; Path=/".parse().unwrap());
        jar.set_cookies_from_headers(&url, &headers);

        // Replace with new value
        let mut headers = HeaderMap::new();
        headers.insert("set-cookie", "name=new; Path=/".parse().unwrap());
        jar.set_cookies_from_headers(&url, &headers);

        assert_eq!(jar.len(), 1);
        let cookies = jar.cookies_for_url(&url);
        assert_eq!(cookies.as_deref(), Some("name=new"));
    }

    #[test]
    fn test_cookie_jar_multiple() {
        let mut jar = CookieJar::new();
        let url = Url::parse("https://example.com/").unwrap();

        let mut headers = HeaderMap::new();
        headers.append("set-cookie", "a=1; Path=/".parse().unwrap());
        headers.append("set-cookie", "b=2; Path=/".parse().unwrap());
        jar.set_cookies_from_headers(&url, &headers);

        assert_eq!(jar.len(), 2);
        let cookies = jar.cookies_for_url(&url).unwrap();
        assert!(cookies.contains("a=1"));
        assert!(cookies.contains("b=2"));
    }

    #[test]
    fn test_candidate_domains() {
        let domains = candidate_domains("sub.example.com");
        assert!(domains.contains(&"sub.example.com".to_string()));
        assert!(domains.contains(&"example.com".to_string()));
    }

    #[test]
    fn test_set_request_cookies_helper() {
        let mut jar = CookieJar::new();
        let url = Url::parse("https://example.com/").unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("set-cookie", "session=abc; Path=/".parse().unwrap());
        jar.set_cookies_from_headers(&url, &headers);

        let mut req_headers = HeaderMap::new();
        set_request_cookies(&url, &mut req_headers, &mut jar);
        assert_eq!(
            req_headers.get("cookie").unwrap().to_str().unwrap(),
            "session=abc",
        );
    }
}
