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
