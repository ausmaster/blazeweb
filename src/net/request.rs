/// Unified HTTP request type (Servo `net_traits::request` + Chromium `ResourceRequest`).
///
/// All resource fetching flows through this type: document loading, script fetching,
/// JS `fetch()` API, and `XMLHttpRequest`. Destination-aware headers (Accept,
/// Sec-Fetch-*) are set automatically based on the request's `Destination`.

use reqwest::header::HeaderMap;
use reqwest::{Method, Url};

// ── Destination ──────────────────────────────────────────────────────────────

/// Resource destination (Chromium `RequestDestination` / Fetch spec §2.2.7).
///
/// Drives per-resource Accept headers, Sec-Fetch-Dest values, and cache behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)] // Spec-complete enum — not all variants used yet
pub enum Destination {
    /// Top-level document navigation.
    Document,
    /// `<script>` element or importScripts().
    Script,
    /// `<link rel="stylesheet">` or `@import`.
    Style,
    /// `<img>`, CSS `background-image`, etc.
    Image,
    /// `@font-face`.
    Font,
    /// JS `fetch()` API.
    Fetch,
    /// `XMLHttpRequest`.
    Xhr,
    /// `<iframe>`.
    IFrame,
    /// JSON destination.
    Json,
    /// No specific destination.
    Empty,
}

impl Destination {
    /// Sec-Fetch-Dest header value (Chromium `RequestDestinationToString`).
    pub fn sec_fetch_dest(&self) -> &'static str {
        match self {
            Self::Document => "document",
            Self::Script => "script",
            Self::Style => "style",
            Self::Image => "image",
            Self::Font => "font",
            Self::Fetch => "empty",
            Self::Xhr => "empty",
            Self::IFrame => "iframe",
            Self::Json => "json",
            Self::Empty => "empty",
        }
    }

    /// Default Accept header value for this destination.
    ///
    /// Document: Chromium's navigation Accept.
    /// Image: Chromium's image Accept (avif/webp/apng/svg).
    /// Style: `text/css,*/*;q=0.1`.
    /// Json: `application/json,*/*;q=0.5`.
    /// Everything else: `*/*`.
    pub fn default_accept(&self) -> &'static str {
        match self {
            Self::Document | Self::IFrame => {
                "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8"
            }
            Self::Image => {
                "image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8"
            }
            Self::Style => "text/css,*/*;q=0.1",
            Self::Json => "application/json,*/*;q=0.5",
            _ => "*/*",
        }
    }

    /// Whether this is a navigation request (Document, IFrame).
    pub fn is_navigation(&self) -> bool {
        matches!(self, Self::Document | Self::IFrame)
    }
}

// ── RequestMode ──────────────────────────────────────────────────────────────

/// Request mode (Fetch spec §2.2.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Spec-complete enum
pub enum RequestMode {
    /// Navigation request — top-level or iframe.
    Navigate,
    /// No CORS required.
    NoCors,
    /// CORS-enabled.
    Cors,
    /// Same-origin only.
    SameOrigin,
}

impl RequestMode {
    /// Sec-Fetch-Mode header value.
    pub fn sec_fetch_mode(&self) -> &'static str {
        match self {
            Self::Navigate => "navigate",
            Self::NoCors => "no-cors",
            Self::Cors => "cors",
            Self::SameOrigin => "same-origin",
        }
    }
}

// ── CredentialsMode ──────────────────────────────────────────────────────────

/// Credentials mode (Fetch spec §2.2.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Spec-complete enum
pub enum CredentialsMode {
    /// Never send credentials.
    Omit,
    /// Send credentials only for same-origin requests.
    SameOrigin,
    /// Always send credentials.
    Include,
}

// ── RedirectMode ─────────────────────────────────────────────────────────────

/// Redirect mode (Fetch spec §2.2.10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Spec-complete enum
pub enum RedirectMode {
    /// Follow redirects automatically (default).
    Follow,
    /// Treat redirects as errors.
    Error,
    /// Return an opaque-redirect filtered response.
    Manual,
}

// ── CacheMode ────────────────────────────────────────────────────────────────

/// Cache mode (Fetch spec §2.2.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Spec-complete enum
pub enum CacheMode {
    /// Normal caching behavior.
    Default,
    /// Always revalidate with the server (add max-age=0).
    NoCache,
    /// Bypass cache entirely (don't read or write).
    NoStore,
    /// Unconditional fetch (add Pragma: no-cache, Cache-Control: no-cache).
    Reload,
    /// Force use of cache, even if stale.
    ForceCache,
    /// Only use cache — network error if not cached.
    OnlyIfCached,
}

// ── Request ──────────────────────────────────────────────────────────────────

/// Maximum number of redirects before returning a network error (Fetch spec §4.3).
pub const MAX_REDIRECTS: u32 = 20;

/// User-Agent string sent with all requests.
pub const USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Blazeweb/0.1";

/// Unified HTTP request.
///
/// Created via convenience constructors (`document()`, `script()`, `fetch_api()`,
/// `xhr()`) which set destination-appropriate defaults. The unified fetch pipeline
/// reads these fields to set Accept, Sec-Fetch-*, and other headers.
#[derive(Debug)]
#[allow(dead_code)] // Spec-complete struct — not all fields used yet
pub struct Request {
    pub method: Method,
    pub url_list: Vec<Url>,
    pub headers: HeaderMap,
    pub body: Option<Vec<u8>>,
    pub destination: Destination,
    pub mode: RequestMode,
    pub credentials_mode: CredentialsMode,
    pub redirect_mode: RedirectMode,
    pub cache_mode: CacheMode,
    pub redirect_count: u32,
    /// Whether this is a user-activated navigation (for Sec-Fetch-User: ?1).
    pub user_activation: bool,
}

impl Request {
    /// The original URL (first in the redirect chain).
    #[allow(dead_code)]
    pub fn url(&self) -> &Url {
        self.url_list.first().expect("url_list must not be empty")
    }

    /// The current URL (last in the redirect chain, after any redirects).
    pub fn current_url(&self) -> &Url {
        self.url_list.last().expect("url_list must not be empty")
    }

    /// Whether the request has been redirected.
    pub fn was_redirected(&self) -> bool {
        self.url_list.len() > 1
    }

    // ── Convenience constructors ─────────────────────────────────────────

    /// Create a request for fetching a top-level HTML document.
    pub fn document(url: Url) -> Self {
        Self {
            method: Method::GET,
            url_list: vec![url],
            headers: HeaderMap::new(),
            body: None,
            destination: Destination::Document,
            mode: RequestMode::Navigate,
            credentials_mode: CredentialsMode::Include,
            redirect_mode: RedirectMode::Follow,
            cache_mode: CacheMode::Default,
            redirect_count: 0,
            user_activation: true,
        }
    }

    /// Create a request for fetching an external `<script src>`.
    pub fn script(url: Url) -> Self {
        Self {
            method: Method::GET,
            url_list: vec![url],
            headers: HeaderMap::new(),
            body: None,
            destination: Destination::Script,
            mode: RequestMode::NoCors,
            credentials_mode: CredentialsMode::SameOrigin,
            redirect_mode: RedirectMode::Follow,
            cache_mode: CacheMode::Default,
            redirect_count: 0,
            user_activation: false,
        }
    }

    /// Create a request for the JS `fetch()` API.
    pub fn fetch_api(url: Url, method: Method) -> Self {
        Self {
            method,
            url_list: vec![url],
            headers: HeaderMap::new(),
            body: None,
            destination: Destination::Fetch,
            mode: RequestMode::Cors,
            credentials_mode: CredentialsMode::SameOrigin,
            redirect_mode: RedirectMode::Follow,
            cache_mode: CacheMode::Default,
            redirect_count: 0,
            user_activation: false,
        }
    }

    /// Create a request for `XMLHttpRequest`.
    pub fn xhr(url: Url, method: Method) -> Self {
        Self {
            method,
            url_list: vec![url],
            headers: HeaderMap::new(),
            body: None,
            destination: Destination::Xhr,
            mode: RequestMode::Cors,
            credentials_mode: CredentialsMode::SameOrigin,
            redirect_mode: RedirectMode::Follow,
            cache_mode: CacheMode::Default,
            redirect_count: 0,
            user_activation: false,
        }
    }
}

/// Header names that are stripped when the request method changes on redirect
/// (POST → GET on 301/302/303). From Chromium's `redirect_util.cc`.
pub const REQUEST_BODY_HEADERS: &[&str] = &[
    "content-encoding",
    "content-language",
    "content-location",
    "content-type",
    "content-length",
    "origin",
];

#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;

