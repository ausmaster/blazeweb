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
        assert!(req.headers.get("user-agent").unwrap().to_str().unwrap().contains("Chrome"));
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

    // ─── data: URL tests ────────────────────────────────────────────────

    #[test]
    fn test_data_url_text_javascript() {
        let url = Url::parse("data:text/javascript,var%20x%20%3D%2042%3B").unwrap();
        let req = Request::script(url);
        let resp = data_fetch(&req);
        assert_eq!(resp.status, 200);
        assert!(!resp.is_network_error());
        assert_eq!(resp.text(), "var x = 42;");
        let ct = resp.headers.get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("javascript"), "content-type should contain javascript, got: {}", ct);
    }

    #[test]
    fn test_data_url_base64() {
        // "hello world" in base64
        let url = Url::parse("data:text/plain;base64,aGVsbG8gd29ybGQ=").unwrap();
        let req = Request::script(url);
        let resp = data_fetch(&req);
        assert_eq!(resp.status, 200);
        assert_eq!(resp.text(), "hello world");
    }

    #[test]
    fn test_data_url_no_mediatype() {
        // No media type → defaults to text/plain;charset=US-ASCII per spec
        let url = Url::parse("data:,Hello").unwrap();
        let req = Request::script(url);
        let resp = data_fetch(&req);
        assert_eq!(resp.status, 200);
        assert_eq!(resp.text(), "Hello");
    }

    #[test]
    fn test_data_url_invalid() {
        // Malformed data URL
        let url = Url::parse("data:").unwrap();
        let req = Request::script(url);
        let resp = data_fetch(&req);
        assert!(resp.is_network_error());
    }

    #[test]
    fn test_data_url_empty_body() {
        let url = Url::parse("data:text/javascript,").unwrap();
        let req = Request::script(url);
        let resp = data_fetch(&req);
        assert_eq!(resp.status, 200);
        assert_eq!(resp.text(), "");
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

    // ─── build_client / configurable TLS tests ─────────────────────────────

    #[test]
    fn test_build_client_default_options() {
        let client = build_client(10, 5, 6, true, true, true, true);
        assert!(client.is_ok());
    }

    #[test]
    fn test_build_client_no_post_quantum() {
        let client = build_client(10, 5, 6, true, true, true, false);
        assert!(client.is_ok());
    }

    #[test]
    fn test_build_client_no_alps() {
        let client = build_client(10, 5, 6, true, false, true, true);
        assert!(client.is_ok());
    }

    #[test]
    fn test_build_client_all_disabled() {
        let client = build_client(10, 5, 6, false, false, false, false);
        assert!(client.is_ok());
    }

    #[test]
    fn test_fetch_context_default_max_per_host() {
        let ctx = FetchContext::new(None);
        assert_eq!(ctx.max_connections_per_host, 6);
        assert!(ctx.client.is_none());
    }

    #[test]
    fn test_fetch_context_with_shared_custom_client() {
        let custom = build_client(5, 2, 3, false, false, false, false).unwrap();
        let ctx = FetchContext::with_shared(
            Some("https://example.com"),
            std::sync::Arc::new(std::sync::Mutex::new(crate::net::cookies::CookieJar::new())),
            std::sync::Arc::new(std::sync::Mutex::new(crate::net::http_cache::HttpCache::new())),
            true,
            true,
            Some(std::sync::Arc::new(custom)),
            3,
        );
        assert!(ctx.client.is_some());
        assert_eq!(ctx.max_connections_per_host, 3);
    }

    #[test]
    fn test_build_client_can_fetch() {
        // Verify custom client can make real HTTPS requests through our pipeline
        let custom = build_client(10, 5, 6, true, true, true, true).unwrap();
        let custom_ctx = FetchContext {
            base_url: Some("https://httpbin.org/html".to_string()),
            cookie_jar: None,
            http_cache: None,
            cache_read: false,
            cache_write: false,
            client: Some(std::sync::Arc::new(custom)),
            max_connections_per_host: 6,
        };
        let url = Url::parse("https://httpbin.org/html").unwrap();
        let mut req = Request::document(url);
        let resp = fetch(&mut req, &custom_ctx);
        assert!(resp.ok(), "Custom client fetch failed: status={} text={}", resp.status, resp.status_text);
        assert!(resp.text().contains("Herman Melville"));
    }
