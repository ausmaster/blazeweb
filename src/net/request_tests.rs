    use super::*;

    #[test]
    fn test_destination_sec_fetch_dest() {
        assert_eq!(Destination::Document.sec_fetch_dest(), "document");
        assert_eq!(Destination::Script.sec_fetch_dest(), "script");
        assert_eq!(Destination::Style.sec_fetch_dest(), "style");
        assert_eq!(Destination::Image.sec_fetch_dest(), "image");
        assert_eq!(Destination::Font.sec_fetch_dest(), "font");
        assert_eq!(Destination::Fetch.sec_fetch_dest(), "empty");
        assert_eq!(Destination::Xhr.sec_fetch_dest(), "empty");
        assert_eq!(Destination::IFrame.sec_fetch_dest(), "iframe");
        assert_eq!(Destination::Json.sec_fetch_dest(), "json");
        assert_eq!(Destination::Empty.sec_fetch_dest(), "empty");
    }

    #[test]
    fn test_destination_default_accept() {
        assert!(Destination::Document.default_accept().starts_with("text/html"));
        assert!(Destination::Image.default_accept().starts_with("image/"));
        assert_eq!(Destination::Style.default_accept(), "text/css,*/*;q=0.1");
        assert_eq!(Destination::Script.default_accept(), "*/*");
        assert_eq!(Destination::Fetch.default_accept(), "*/*");
    }

    #[test]
    fn test_request_document() {
        let url = Url::parse("https://example.com").unwrap();
        let req = Request::document(url.clone());
        assert_eq!(req.method, Method::GET);
        assert_eq!(req.destination, Destination::Document);
        assert_eq!(req.mode, RequestMode::Navigate);
        assert_eq!(req.credentials_mode, CredentialsMode::Include);
        assert!(req.user_activation);
        assert_eq!(req.url(), &url);
        assert_eq!(req.current_url(), &url);
        assert!(!req.was_redirected());
    }

    #[test]
    fn test_request_fetch_api() {
        let url = Url::parse("https://api.example.com/data").unwrap();
        let req = Request::fetch_api(url, Method::POST);
        assert_eq!(req.method, Method::POST);
        assert_eq!(req.destination, Destination::Fetch);
        assert_eq!(req.mode, RequestMode::Cors);
        assert!(!req.user_activation);
    }

    #[test]
    fn test_request_redirect_tracking() {
        let url1 = Url::parse("https://example.com/a").unwrap();
        let url2 = Url::parse("https://example.com/b").unwrap();
        let mut req = Request::document(url1.clone());
        req.url_list.push(url2.clone());
        req.redirect_count = 1;
        assert_eq!(req.url(), &url1);
        assert_eq!(req.current_url(), &url2);
        assert!(req.was_redirected());
    }
