    use super::*;

    #[test]
    fn test_v8_initializes() {
        ensure_v8_initialized();
        ensure_v8_initialized(); // Second call should be fine
    }

    #[test]
    fn test_extract_scripts_empty() {
        let arena = crate::dom::treesink::parse("<html><body><p>No scripts</p></body></html>");
        let scripts = extract_scripts(&arena);
        assert!(scripts.is_empty());
    }

    #[test]
    fn test_extract_scripts_inline() {
        let arena = crate::dom::treesink::parse(
            "<html><body><script>var x = 1;</script></body></html>",
        );
        let scripts = extract_scripts(&arena);
        assert_eq!(scripts.len(), 1);
        assert!(matches!(&scripts[0].source, ScriptSource::Inline(s) if s == "var x = 1;"));
        assert_eq!(scripts[0].name, "inline-script-0");
    }

    #[test]
    fn test_extract_scripts_multiple() {
        let arena = crate::dom::treesink::parse(
            "<html><body>\
             <script>var a = 1;</script>\
             <script>var b = 2;</script>\
             <script>var c = 3;</script>\
             </body></html>",
        );
        let scripts = extract_scripts(&arena);
        assert_eq!(scripts.len(), 3);
        assert!(matches!(&scripts[0].source, ScriptSource::Inline(s) if s == "var a = 1;"));
        assert!(matches!(&scripts[1].source, ScriptSource::Inline(s) if s == "var b = 2;"));
        assert!(matches!(&scripts[2].source, ScriptSource::Inline(s) if s == "var c = 3;"));
    }

    #[test]
    fn test_extract_scripts_skips_non_js_type() {
        let arena = crate::dom::treesink::parse(
            r#"<html><body><script type="application/json">{"key": "value"}</script></body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert!(scripts.is_empty());
    }

    #[test]
    fn test_extract_scripts_includes_type_module() {
        let arena = crate::dom::treesink::parse(
            r#"<html><body><script type="module">import x from './x';</script></body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert_eq!(scripts.len(), 1);
        assert!(scripts[0].is_module);
        assert!(matches!(&scripts[0].source, ScriptSource::Inline(s) if s.contains("import")));
    }

    #[test]
    fn test_extract_scripts_external() {
        let arena = crate::dom::treesink::parse(
            r#"<html><body><script src="app.js"></script></body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert_eq!(scripts.len(), 1);
        assert!(matches!(&scripts[0].source, ScriptSource::External(s) if s == "app.js"));
    }

    #[test]
    fn test_extract_scripts_mixed_order() {
        let arena = crate::dom::treesink::parse(
            r#"<html><body>
            <script>var a = 1;</script>
            <script src="lib.js"></script>
            <script>var b = 2;</script>
            </body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert_eq!(scripts.len(), 3);
        assert!(matches!(&scripts[0].source, ScriptSource::Inline(s) if s == "var a = 1;"));
        assert!(matches!(&scripts[1].source, ScriptSource::External(s) if s == "lib.js"));
        assert!(matches!(&scripts[2].source, ScriptSource::Inline(s) if s == "var b = 2;"));
    }

    // ─── Module Script Extraction ───────────────────────────────────────

    #[test]
    fn test_extract_module_script_inline() {
        let arena = crate::dom::treesink::parse(
            r#"<html><body><script type="module">const x = 1;</script></body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert_eq!(scripts.len(), 1);
        assert!(scripts[0].is_module);
        assert!(matches!(&scripts[0].source, ScriptSource::Inline(s) if s == "const x = 1;"));
    }

    #[test]
    fn test_extract_module_script_external() {
        let arena = crate::dom::treesink::parse(
            r#"<html><body><script type="module" src="app.mjs"></script></body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert_eq!(scripts.len(), 1);
        assert!(scripts[0].is_module);
        assert!(matches!(&scripts[0].source, ScriptSource::External(s) if s == "app.mjs"));
    }

    #[test]
    fn test_extract_classic_and_module_scripts() {
        let arena = crate::dom::treesink::parse(
            r#"<html><body>
            <script>var a = 1;</script>
            <script type="module">const b = 2;</script>
            <script>var c = 3;</script>
            <script type="module">const d = 4;</script>
            </body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert_eq!(scripts.len(), 4);
        assert!(!scripts[0].is_module);
        assert!(scripts[1].is_module);
        assert!(!scripts[2].is_module);
        assert!(scripts[3].is_module);
    }

    #[test]
    fn test_extract_nomodule_skipped() {
        let arena = crate::dom::treesink::parse(
            r#"<html><body><script nomodule>var x = 1;</script></body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert!(scripts.is_empty(), "nomodule scripts should be skipped");
    }

    #[test]
    fn test_extract_nomodule_with_module_sibling() {
        let arena = crate::dom::treesink::parse(
            r#"<html><body>
            <script nomodule>var fallback = 1;</script>
            <script type="module">const modern = 2;</script>
            </body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert_eq!(scripts.len(), 1, "should only have the module script");
        assert!(scripts[0].is_module);
    }

    // ─── Module Extraction: Edge Cases ───────────────────────────────────

    #[test]
    fn test_extract_type_module_case_sensitive() {
        // HTML spec: type attribute is case-sensitive for scripts
        let arena = crate::dom::treesink::parse(
            r#"<html><body><script type="Module">var x = 1;</script></body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        // "Module" (capital M) is not "module" — should be skipped
        assert!(scripts.is_empty(), "type='Module' should not match, got {} scripts", scripts.len());
    }

    #[test]
    fn test_extract_nomodule_external_skipped() {
        let arena = crate::dom::treesink::parse(
            r#"<html><body><script nomodule src="legacy.js"></script></body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert!(scripts.is_empty(), "nomodule external scripts should be skipped");
    }

    #[test]
    fn test_extract_nomodule_with_empty_value() {
        // <script nomodule=""> — boolean attribute with explicit empty value
        let arena = crate::dom::treesink::parse(
            r#"<html><body><script nomodule="">var x = 1;</script></body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert!(scripts.is_empty(), "nomodule='' should still be skipped");
    }

    #[test]
    fn test_extract_module_empty_body() {
        // <script type="module"></script> — empty module should be skipped
        let arena = crate::dom::treesink::parse(
            r#"<html><body><script type="module"></script></body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert!(scripts.is_empty(), "empty module script should not be extracted");
    }

    #[test]
    fn test_extract_module_whitespace_only() {
        // Module with only whitespace — should be extracted (whitespace is valid JS)
        let arena = crate::dom::treesink::parse(
            r#"<html><body><script type="module">   </script></body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert_eq!(scripts.len(), 1, "whitespace-only module should be extracted");
        assert!(scripts[0].is_module);
    }

    #[test]
    fn test_extract_importmap_skipped() {
        // <script type="importmap"> should NOT be extracted as JS
        let arena = crate::dom::treesink::parse(
            r#"<html><body><script type="importmap">{"imports":{}}</script></body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert!(scripts.is_empty(), "importmap scripts should be skipped");
    }

    #[test]
    fn test_extract_json_still_skipped_with_modules() {
        let arena = crate::dom::treesink::parse(
            r#"<html><body>
            <script type="application/json">{"key":"val"}</script>
            <script type="module">const x = 1;</script>
            </body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert_eq!(scripts.len(), 1);
        assert!(scripts[0].is_module);
    }

    #[test]
    fn test_extract_classic_is_not_module() {
        let arena = crate::dom::treesink::parse(
            r#"<html><body>
            <script>var x = 1;</script>
            <script type="text/javascript">var y = 2;</script>
            <script type="application/javascript">var z = 3;</script>
            </body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert_eq!(scripts.len(), 3);
        for s in &scripts {
            assert!(!s.is_module, "classic scripts should have is_module=false");
        }
    }

    #[test]
    fn test_extract_module_external_has_is_module() {
        let arena = crate::dom::treesink::parse(
            r#"<html><body>
            <script src="classic.js"></script>
            <script type="module" src="modern.mjs"></script>
            </body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert_eq!(scripts.len(), 2);
        assert!(!scripts[0].is_module);
        assert!(scripts[1].is_module);
    }
