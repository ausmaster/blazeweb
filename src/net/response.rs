/// Unified HTTP response type (Servo `net_traits::response`).
///
/// Returned by the unified fetch pipeline. Contains status, headers, body,
/// and the full redirect chain (`url_list`).

use reqwest::header::HeaderMap;
use reqwest::Url;

// ── ResponseType ─────────────────────────────────────────────────────────────

/// Response type (Fetch spec §2.2.6).
///
/// Controls header filtering for JS-visible responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseType {
    /// Same-origin response — all headers visible except Set-Cookie.
    Basic,
    /// Cross-origin CORS response — only CORS-safelisted headers visible.
    Cors,
    /// Opaque response — no access to headers, status, or body.
    Opaque,
    /// Network error (DNS failure, timeout, etc.).
    Error,
}

// ── Response ─────────────────────────────────────────────────────────────────

/// Unified HTTP response.
///
/// Created by the fetch pipeline's `network_fetch()` and enriched by
/// `http_redirect_fetch()` (url_list), `http_fetch()` (cookies), and
/// `http_network_or_cache_fetch()` (cache state).
#[derive(Debug)]
pub struct Response {
    pub response_type: ResponseType,
    pub status: u16,
    pub status_text: String,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
    /// Full redirect chain: first = original URL, last = final URL.
    pub url_list: Vec<Url>,
}

impl Response {
    /// Create a network error response.
    pub fn network_error(reason: &str) -> Self {
        Self {
            response_type: ResponseType::Error,
            status: 0,
            status_text: reason.to_string(),
            headers: HeaderMap::new(),
            body: Vec::new(),
            url_list: Vec::new(),
        }
    }

    /// Whether the response is a network error.
    pub fn is_network_error(&self) -> bool {
        self.response_type == ResponseType::Error
    }

    /// Whether the status is in the 200-299 range.
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The final URL after all redirects (last in url_list).
    pub fn final_url(&self) -> Option<&Url> {
        self.url_list.last()
    }

    /// Whether the response was redirected (more than one URL in url_list).
    pub fn was_redirected(&self) -> bool {
        self.url_list.len() > 1
    }

    /// Response body as UTF-8 text (lossy).
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// Whether the status is a redirect (301, 302, 303, 307, 308).
    pub fn is_redirect(&self) -> bool {
        matches!(self.status, 301 | 302 | 303 | 307 | 308)
    }

    /// Get the Location header value, resolved against the current URL.
    pub fn location_url(&self, base: &Url) -> Option<Url> {
        let location = self.headers.get("location")?.to_str().ok()?;
        base.join(location).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_error() {
        let resp = Response::network_error("connection refused");
        assert!(resp.is_network_error());
        assert!(!resp.ok());
        assert_eq!(resp.status, 0);
        assert_eq!(resp.status_text, "connection refused");
        assert!(resp.body.is_empty());
        assert!(resp.final_url().is_none());
    }

    #[test]
    fn test_ok_response() {
        let resp = Response {
            response_type: ResponseType::Basic,
            status: 200,
            status_text: "OK".into(),
            headers: HeaderMap::new(),
            body: b"hello".to_vec(),
            url_list: vec![Url::parse("https://example.com").unwrap()],
        };
        assert!(resp.ok());
        assert!(!resp.is_network_error());
        assert!(!resp.was_redirected());
        assert_eq!(resp.text(), "hello");
        assert_eq!(resp.final_url().unwrap().as_str(), "https://example.com/");
    }

    #[test]
    fn test_redirect_response() {
        let mut headers = HeaderMap::new();
        headers.insert("location", "/new".parse().unwrap());
        let base = Url::parse("https://example.com/old").unwrap();
        let resp = Response {
            response_type: ResponseType::Basic,
            status: 301,
            status_text: "Moved Permanently".into(),
            headers,
            body: Vec::new(),
            url_list: vec![base.clone()],
        };
        assert!(resp.is_redirect());
        let loc = resp.location_url(&base).unwrap();
        assert_eq!(loc.as_str(), "https://example.com/new");
    }

    #[test]
    fn test_was_redirected() {
        let resp = Response {
            response_type: ResponseType::Basic,
            status: 200,
            status_text: "OK".into(),
            headers: HeaderMap::new(),
            body: Vec::new(),
            url_list: vec![
                Url::parse("https://example.com/a").unwrap(),
                Url::parse("https://example.com/b").unwrap(),
            ],
        };
        assert!(resp.was_redirected());
    }
}
