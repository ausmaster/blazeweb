    use super::*;

    fn make_request(url: &str) -> Request {
        let parsed = Url::parse(url).unwrap();
        Request::script(parsed)
    }

    fn make_response(status: u16, headers: Vec<(&str, &str)>, body: &str) -> Response {
        let mut hm = HeaderMap::new();
        for (k, v) in headers {
            hm.insert(
                k.parse::<wreq::header::HeaderName>().unwrap(),
                HeaderValue::from_str(v).unwrap(),
            );
        }
        Response {
            response_type: ResponseType::Basic,
            status,
            status_text: "OK".into(),
            headers: hm,
            body: body.as_bytes().to_vec(),
            url_list: vec![Url::parse("https://example.com/resource").unwrap()],
        }
    }

    #[test]
    fn test_cache_store_and_retrieve() {
        let mut cache = HttpCache::new();
        let req = make_request("https://example.com/resource");
        let resp = make_response(200, vec![("cache-control", "max-age=3600")], "hello");

        cache.store(&req, &resp);
        assert_eq!(cache.len(), 1);

        let cached = cache.construct_response(&req);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().text(), "hello");
    }

    #[test]
    fn test_cache_no_store() {
        let mut cache = HttpCache::new();
        let req = make_request("https://example.com/resource");
        let resp = make_response(200, vec![("cache-control", "no-store")], "secret");

        cache.store(&req, &resp);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_cache_miss() {
        let cache = HttpCache::new();
        let req = make_request("https://example.com/resource");
        assert!(cache.construct_response(&req).is_none());
    }

    #[test]
    fn test_cache_post_not_cached() {
        let mut cache = HttpCache::new();
        let url = Url::parse("https://example.com/resource").unwrap();
        let req = Request::fetch_api(url, Method::POST);
        let resp = make_response(200, vec![("cache-control", "max-age=3600")], "post-response");
        cache.store(&req, &resp);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_cache_invalidation() {
        let mut cache = HttpCache::new();
        let req = make_request("https://example.com/resource");
        let resp = make_response(200, vec![("cache-control", "max-age=3600")], "hello");
        cache.store(&req, &resp);
        assert_eq!(cache.len(), 1);

        cache.invalidate(&Url::parse("https://example.com/resource").unwrap());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_revalidation_headers() {
        let mut cache = HttpCache::new();
        let req = make_request("https://example.com/resource");
        let resp = make_response(200, vec![
            ("cache-control", "max-age=0"),
            ("etag", "\"abc123\""),
            ("last-modified", "Tue, 15 Nov 1994 12:45:26 GMT"),
        ], "stale");

        cache.store(&req, &resp);

        let reval_headers = cache.get_revalidation_headers(&req);
        assert!(reval_headers.is_some());
        let h = reval_headers.unwrap();
        assert_eq!(h.get("if-none-match").unwrap(), "\"abc123\"");
        assert!(h.contains_key("if-modified-since"));
    }

    #[test]
    fn test_refresh_304() {
        let mut cache = HttpCache::new();
        let req = make_request("https://example.com/resource");
        let resp = make_response(200, vec![
            ("cache-control", "max-age=0"),
            ("etag", "\"abc123\""),
        ], "original body");

        cache.store(&req, &resp);

        // Simulate 304 response with updated Cache-Control
        let response_304 = make_response(304, vec![
            ("cache-control", "max-age=3600"),
            ("etag", "\"abc123\""),
        ], "");

        let refreshed = cache.refresh(&req, &response_304);
        assert!(refreshed.is_some());
        assert_eq!(refreshed.unwrap().text(), "original body");
    }

    #[test]
    fn test_vary_matching() {
        let mut cache = HttpCache::new();

        let url = Url::parse("https://example.com/api").unwrap();
        let mut req = Request::script(url.clone());
        req.headers.insert("accept", HeaderValue::from_static("application/json"));

        let resp = make_response(200, vec![
            ("cache-control", "max-age=3600"),
            ("vary", "accept"),
        ], "json response");

        cache.store(&req, &resp);

        // Same accept → should hit
        let mut req2 = Request::script(url.clone());
        req2.headers.insert("accept", HeaderValue::from_static("application/json"));
        assert!(cache.construct_response(&req2).is_some());

        // Different accept → should miss
        let mut req3 = Request::script(url);
        req3.headers.insert("accept", HeaderValue::from_static("text/html"));
        assert!(cache.construct_response(&req3).is_none());
    }

    #[test]
    fn test_compute_freshness_max_age() {
        let mut headers = HeaderMap::new();
        headers.insert("cache-control", HeaderValue::from_static("max-age=3600"));
        let fl = compute_freshness_lifetime(&headers);
        assert!(fl >= Duration::from_secs(3599)); // Allow small timing variance
    }

    #[test]
    fn test_compute_freshness_no_cache() {
        let mut headers = HeaderMap::new();
        headers.insert("cache-control", HeaderValue::from_static("no-cache"));
        let fl = compute_freshness_lifetime(&headers);
        assert_eq!(fl, Duration::ZERO);
    }

    #[test]
    fn test_is_cacheable_by_default() {
        assert!(is_cacheable_by_default(200));
        assert!(is_cacheable_by_default(301));
        assert!(is_cacheable_by_default(404));
        assert!(!is_cacheable_by_default(201));
        assert!(!is_cacheable_by_default(500));
    }
