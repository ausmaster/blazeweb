/// Async HTTP client for fetching external `<script src="...">` resources.
///
/// Uses a shared tokio runtime and reqwest client with HTTP/2, connection
/// pooling, and parallel fetching. Scripts are fetched concurrently but
/// executed in document order by the caller.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use reqwest::{Client, Url};
use tokio::runtime::Runtime;
use tokio::task::JoinSet;

use crate::error::EngineError;

/// Thread-safe script cache: resolved URL string → fetched script text.
pub type ScriptCache = Mutex<HashMap<String, String>>;

/// Controls cache behavior for a single fetch operation.
pub struct CacheOpts<'a> {
    pub cache: &'a ScriptCache,
    pub read: bool,
    pub write: bool,
}

static RT: LazyLock<Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("failed to create tokio runtime")
});

static CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(10)
        .build()
        .expect("failed to create HTTP client")
});

/// Resolve a possibly-relative `src` attribute against a base URL.
pub fn resolve_url(src: &str, base_url: Option<&str>) -> Result<Url, EngineError> {
    // Try absolute first
    if let Ok(url) = Url::parse(src) {
        return Ok(url);
    }
    // Relative — need a base
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

/// Fetch multiple scripts in parallel. Returns `(index, result)` pairs
/// so the caller can slot results back into document order.
pub fn fetch_scripts(urls: Vec<(usize, Url)>) -> Vec<(usize, Result<String, EngineError>)> {
    if urls.is_empty() {
        return vec![];
    }
    RT.block_on(fetch_all(urls))
}

async fn fetch_all(urls: Vec<(usize, Url)>) -> Vec<(usize, Result<String, EngineError>)> {
    let mut set = JoinSet::new();
    for (idx, url) in urls {
        set.spawn(async move {
            let result = fetch_one(&url).await;
            (idx, result)
        });
    }

    let mut results = Vec::new();
    while let Some(res) = set.join_next().await {
        match res {
            Ok(pair) => results.push(pair),
            Err(e) => {
                // JoinError means the task panicked — shouldn't happen
                log::error!("fetch task panicked: {e}");
            }
        }
    }
    results
}

/// Fetch scripts with cache support. Checks cache first (if read enabled),
/// fetches missing scripts in parallel, and stores results (if write enabled).
pub fn fetch_scripts_cached(
    urls: Vec<(usize, Url)>,
    opts: &CacheOpts,
) -> Vec<(usize, Result<String, EngineError>)> {
    if urls.is_empty() {
        return vec![];
    }

    let mut results = Vec::new();
    let mut to_fetch = Vec::new();

    // Check cache for hits (if read enabled)
    if opts.read {
        let cache_map = opts.cache.lock().unwrap();
        for (idx, url) in &urls {
            if let Some(text) = cache_map.get(url.as_str()) {
                results.push((*idx, Ok(text.clone())));
            } else {
                to_fetch.push((*idx, url.clone()));
            }
        }
    } else {
        to_fetch = urls.iter().map(|(i, u)| (*i, u.clone())).collect();
    }

    // Fetch missing scripts in parallel
    if !to_fetch.is_empty() {
        // Build index → URL mapping for cache writes
        let idx_to_url: HashMap<usize, String> = to_fetch
            .iter()
            .map(|(idx, url)| (*idx, url.as_str().to_owned()))
            .collect();

        let fetched = fetch_scripts(to_fetch);

        if opts.write {
            let mut cache_map = opts.cache.lock().unwrap();
            for (idx, result) in &fetched {
                if let Ok(text) = result {
                    if let Some(url_str) = idx_to_url.get(idx) {
                        cache_map.insert(url_str.clone(), text.clone());
                    }
                }
            }
        }

        results.extend(fetched);
    }

    results
}

async fn fetch_one(url: &Url) -> Result<String, EngineError> {
    let url_str = url.as_str().to_owned();
    let resp = CLIENT.get(url.clone()).send().await.map_err(|e| EngineError::Network {
        url: url_str.clone(),
        reason: e.to_string(),
    })?;

    if !resp.status().is_success() {
        return Err(EngineError::Network {
            url: url_str,
            reason: format!("HTTP {}", resp.status()),
        });
    }

    resp.text().await.map_err(|e| EngineError::Network {
        url: url_str,
        reason: e.to_string(),
    })
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
}
