    use super::*;

    // ─── resolve_module_specifier ──────────────────────────────────────

    #[test]
    fn test_resolve_absolute_https_url() {
        let result = resolve_module_specifier(
            "https://cdn.example.com/lib.mjs",
            "https://example.com/app.mjs",
        );
        assert_eq!(result.unwrap(), "https://cdn.example.com/lib.mjs");
    }

    #[test]
    fn test_resolve_absolute_http_url() {
        let result = resolve_module_specifier(
            "http://cdn.example.com/lib.mjs",
            "https://example.com/app.mjs",
        );
        assert_eq!(result.unwrap(), "http://cdn.example.com/lib.mjs");
    }

    #[test]
    fn test_resolve_absolute_file_url() {
        let result = resolve_module_specifier(
            "file:///tmp/lib.mjs",
            "https://example.com/app.mjs",
        );
        assert_eq!(result.unwrap(), "file:///tmp/lib.mjs");
    }

    #[test]
    fn test_resolve_relative_dot_slash() {
        let result = resolve_module_specifier(
            "./utils.mjs",
            "https://example.com/js/app.mjs",
        );
        assert_eq!(result.unwrap(), "https://example.com/js/utils.mjs");
    }

    #[test]
    fn test_resolve_relative_dot_dot_slash() {
        let result = resolve_module_specifier(
            "../lib/utils.mjs",
            "https://example.com/js/app.mjs",
        );
        assert_eq!(result.unwrap(), "https://example.com/lib/utils.mjs");
    }

    #[test]
    fn test_resolve_relative_leading_slash() {
        let result = resolve_module_specifier(
            "/lib/utils.mjs",
            "https://example.com/js/app.mjs",
        );
        assert_eq!(result.unwrap(), "https://example.com/lib/utils.mjs");
    }

    #[test]
    fn test_resolve_bare_specifier_fails() {
        let result = resolve_module_specifier(
            "lodash",
            "https://example.com/app.mjs",
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("bare module specifier"), "got: {}", err);
        assert!(err.contains("lodash"), "got: {}", err);
    }

    #[test]
    fn test_resolve_bare_specifier_with_slash_fails() {
        let result = resolve_module_specifier(
            "lodash/fp",
            "https://example.com/app.mjs",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("bare module specifier"));
    }

    #[test]
    fn test_resolve_relative_with_query_string() {
        let result = resolve_module_specifier(
            "./lib.mjs?v=2",
            "https://example.com/js/app.mjs",
        );
        assert_eq!(result.unwrap(), "https://example.com/js/lib.mjs?v=2");
    }

    #[test]
    fn test_resolve_relative_with_hash() {
        let result = resolve_module_specifier(
            "./lib.mjs#section",
            "https://example.com/js/app.mjs",
        );
        assert_eq!(result.unwrap(), "https://example.com/js/lib.mjs#section");
    }

    #[test]
    fn test_resolve_invalid_base_url_fails() {
        let result = resolve_module_specifier(
            "./lib.mjs",
            "not-a-url",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid base URL"));
    }

    #[test]
    fn test_resolve_deep_relative_path() {
        let result = resolve_module_specifier(
            "../../shared/utils.mjs",
            "https://example.com/src/js/components/app.mjs",
        );
        assert_eq!(result.unwrap(), "https://example.com/src/shared/utils.mjs");
    }

    #[test]
    fn test_resolve_dot_slash_index() {
        let result = resolve_module_specifier(
            "./",
            "https://example.com/js/app.mjs",
        );
        // Should resolve to the directory
        assert_eq!(result.unwrap(), "https://example.com/js/");
    }

    #[test]
    fn test_resolve_preserves_trailing_slash() {
        let result = resolve_module_specifier(
            "./subdir/",
            "https://example.com/js/app.mjs",
        );
        assert_eq!(result.unwrap(), "https://example.com/js/subdir/");
    }

    #[test]
    fn test_resolve_absolute_url_ignores_base() {
        // Even with a weird base, absolute URL should resolve as-is
        let result = resolve_module_specifier(
            "https://cdn.example.com/lib.mjs",
            "about:blank",
        );
        assert_eq!(result.unwrap(), "https://cdn.example.com/lib.mjs");
    }

    #[test]
    fn test_resolve_empty_specifier_fails() {
        let result = resolve_module_specifier(
            "",
            "https://example.com/app.mjs",
        );
        // Empty string is a bare specifier
        assert!(result.is_err());
    }

    // ─── ModuleMap unit tests ──────────────────────────────────────────

    #[test]
    fn test_module_map_new_is_empty() {
        let map = ModuleMap::new();
        assert!(map.modules.is_empty());
        assert!(map.identity_to_url.is_empty());
    }

    #[test]
    fn test_module_map_get_missing_returns_none() {
        let map = ModuleMap::new();
        assert!(map.get("https://example.com/missing.mjs").is_none());
    }

    #[test]
    fn test_module_map_url_for_identity_missing() {
        let map = ModuleMap::new();
        assert!(map.url_for_identity(12345).is_none());
    }
