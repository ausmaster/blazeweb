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
