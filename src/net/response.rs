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
#[path = "response_tests.rs"]
mod tests;
