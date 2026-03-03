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
    fn test_extract_scripts_skips_type_module() {
        let arena = crate::dom::treesink::parse(
            r#"<html><body><script type="module">import x from './x';</script></body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert!(scripts.is_empty());
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

    #[test]
    fn test_execute_noop_script() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><script>var x = 1;</script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        assert!(errors.is_empty());
    }

    #[test]
    fn test_execute_no_scripts_fast_path() {
        let mut arena =
            crate::dom::treesink::parse("<html><body><p>Hello</p></body></html>");
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        assert!(errors.is_empty());
    }

    #[test]
    fn test_execute_syntax_error() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><script>function {</script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("SyntaxError"));
    }

    #[test]
    fn test_execute_runtime_error() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><script>undefined.foo</script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("TypeError"));
    }

    #[test]
    fn test_multiple_scripts_sequential() {
        // First script sets a global, second reads it
        let mut arena = crate::dom::treesink::parse(
            "<html><body>\
             <script>var shared = 42;</script>\
             <script>if (shared !== 42) throw new Error('not shared');</script>\
             </body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    #[test]
    fn test_e2e_create_and_set_textcontent() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"container\"></div><script>\
             var el = document.createElement('span');\
             el.textContent = 'dynamic';\
             document.getElementById('container').appendChild(el);\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains("<span>dynamic</span>"), "got: {}", html);
    }

    #[test]
    fn test_e2e_innerhtml_set() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"target\">old</div><script>\
             document.getElementById('target').innerHTML = '<b>bold</b>';\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains("<b>bold</b>"), "got: {}", html);
        assert!(!html.contains(">old<"), "got: {}", html);
    }

    // ─── querySelector / querySelectorAll ─────────────────────────────────

    #[test]
    fn test_e2e_query_selector() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><p class=\"target\">found</p><div id=\"result\"></div><script>\
             var el = document.querySelector('.target');\
             document.getElementById('result').textContent = el ? el.textContent : 'null';\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">found<"), "got: {}", html);
    }

    #[test]
    fn test_e2e_query_selector_all() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><p class=\"item\">a</p><p class=\"item\">b</p><div id=\"result\"></div><script>\
             var els = document.querySelectorAll('.item');\
             document.getElementById('result').textContent = els.length.toString();\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">2<"), "got: {}", html);
    }

    #[test]
    fn test_e2e_element_matches() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"target\" class=\"foo\"></div><div id=\"result\"></div><script>\
             var el = document.getElementById('target');\
             document.getElementById('result').textContent = el.matches('.foo').toString();\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">true<"), "got: {}", html);
    }

    #[test]
    fn test_e2e_element_closest() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div class=\"outer\"><span id=\"target\">X</span></div><div id=\"result\"></div><script>\
             var el = document.getElementById('target');\
             var c = el.closest('.outer');\
             document.getElementById('result').textContent = c ? c.className : 'null';\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">outer<"), "got: {}", html);
    }

    // ─── Timers ──────────────────────────────────────────────────────────

    #[test]
    fn test_e2e_set_timeout() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"result\">before</div><script>\
             setTimeout(function() { document.getElementById('result').textContent = 'after'; }, 0);\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">after<"), "got: {}", html);
    }

    #[test]
    fn test_e2e_clear_timeout() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"result\">unchanged</div><script>\
             var id = setTimeout(function() { document.getElementById('result').textContent = 'changed'; }, 0);\
             clearTimeout(id);\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">unchanged<"), "got: {}", html);
    }

    #[test]
    fn test_e2e_timer_ordering() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"result\"></div><script>\
             var order = [];\
             setTimeout(function() { order.push('b'); }, 100);\
             setTimeout(function() { order.push('a'); }, 0);\
             setTimeout(function() { order.push('c'); document.getElementById('result').textContent = order.join(','); }, 200);\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">a,b,c<"), "got: {}", html);
    }

    #[test]
    fn test_e2e_request_animation_frame() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"result\">before</div><script>\
             requestAnimationFrame(function() { document.getElementById('result').textContent = 'after'; });\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">after<"), "got: {}", html);
    }

    #[test]
    fn test_e2e_nested_timer() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"result\">0</div><script>\
             setTimeout(function() { \
                 setTimeout(function() { document.getElementById('result').textContent = '2'; }, 0);\
             }, 0);\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">2<"), "got: {}", html);
    }

    // ─── Events ──────────────────────────────────────────────────────────

    #[test]
    fn test_e2e_dom_content_loaded() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"result\">pending</div><script>\
             document.addEventListener('DOMContentLoaded', function() {\
                 document.getElementById('result').textContent = 'loaded';\
             });\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">loaded<"), "got: {}", html);
    }

    #[test]
    fn test_e2e_window_dom_content_loaded() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"result\">pending</div><script>\
             window.addEventListener('DOMContentLoaded', function() {\
                 document.getElementById('result').textContent = 'window-loaded';\
             });\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">window-loaded<"), "got: {}", html);
    }

    #[test]
    fn test_e2e_remove_event_listener() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"result\">not called</div><script>\
             function handler() { document.getElementById('result').textContent = 'called'; }\
             document.addEventListener('DOMContentLoaded', handler);\
             document.removeEventListener('DOMContentLoaded', handler);\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">not called<"), "got: {}", html);
    }

    // ─── classList ───────────────────────────────────────────────────────

    #[test]
    fn test_e2e_classlist_add() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"target\"></div><script>\
             document.getElementById('target').classList.add('foo', 'bar');\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains("class=\"foo bar\""), "got: {}", html);
    }

    #[test]
    fn test_e2e_classlist_remove() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"target\" class=\"foo bar baz\"></div><script>\
             document.getElementById('target').classList.remove('bar');\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains("class=\"foo baz\""), "got: {}", html);
    }

    #[test]
    fn test_e2e_classlist_toggle() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"target\" class=\"foo bar\"></div><div id=\"result\"></div><script>\
             var el = document.getElementById('target');\
             var r = el.classList.toggle('bar');\
             document.getElementById('result').textContent = r.toString();\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains("class=\"foo\""), "got: {}", html);
        assert!(html.contains(">false<"), "got: {}", html);
    }

    // ─── Style ───────────────────────────────────────────────────────────

    #[test]
    fn test_e2e_style_set() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"target\"></div><script>\
             document.getElementById('target').style.display = 'none';\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains("style=\"display: none;\"") || html.contains("style=\"display:none\""),
            "expected style attribute with display:none. got: {}", html);
    }

    #[test]
    fn test_e2e_style_read() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"target\" style=\"color: red\"></div><div id=\"result\"></div><script>\
             document.getElementById('result').textContent = document.getElementById('target').style.color;\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">red<"), "got: {}", html);
    }

    // ─── Window/Document Stubs ───────────────────────────────────────────

    #[test]
    fn test_e2e_local_storage() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"result\"></div><script>\
             localStorage.setItem('key', 'value');\
             document.getElementById('result').textContent = localStorage.getItem('key');\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">value<"), "got: {}", html);
    }

    #[test]
    fn test_e2e_navigator() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"result\"></div><script>\
             document.getElementById('result').textContent = typeof navigator.userAgent;\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">string<"), "got: {}", html);
    }

    #[test]
    fn test_e2e_document_ready_state() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"result\"></div><script>\
             document.getElementById('result').textContent = document.readyState;\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">complete<"), "got: {}", html);
    }

    #[test]
    fn test_e2e_atob_btoa() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"result\"></div><script>\
             document.getElementById('result').textContent = atob(btoa('hello'));\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">hello<"), "got: {}", html);
    }

    // ─── Misc DOM Methods ────────────────────────────────────────────────

    #[test]
    fn test_e2e_replace_child() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"parent\"><span id=\"old\">old</span></div><script>\
             var p = document.getElementById('parent');\
             var old = document.getElementById('old');\
             var n = document.createElement('em');\
             n.textContent = 'new';\
             p.replaceChild(n, old);\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains("<em>new</em>"), "got: {}", html);
    }

    #[test]
    fn test_e2e_insert_adjacent_html() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"target\">mid</div><script>\
             var el = document.getElementById('target');\
             el.insertAdjacentHTML('afterbegin', '<b>start</b>');\
             el.insertAdjacentHTML('beforeend', '<i>end</i>');\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains("<b>start</b>"), "got: {}", html);
        assert!(html.contains("<i>end</i>"), "got: {}", html);
    }

    #[test]
    fn test_e2e_append_prepend() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"target\"><span>mid</span></div><script>\
             var el = document.getElementById('target');\
             var first = document.createElement('b');\
             first.textContent = 'first';\
             var last = document.createElement('i');\
             last.textContent = 'last';\
             el.prepend(first);\
             el.append(last);\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains("<b>first</b><span>mid</span><i>last</i>"), "got: {}", html);
    }

    #[test]
    fn test_e2e_event_constructor() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"result\"></div><script>\
             var e = new Event('test', { bubbles: true });\
             document.getElementById('result').textContent = e.type + ',' + e.bubbles;\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">test,true<"), "got: {}", html);
    }

    #[test]
    fn test_e2e_custom_event() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"result\"></div><script>\
             var e = new CustomEvent('myevent', { detail: { foo: 'bar' } });\
             document.getElementById('result').textContent = e.type + ',' + e.detail.foo;\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">myevent,bar<"), "got: {}", html);
    }

    #[test]
    fn test_e2e_url_constructor() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"result\"></div><script>\
             var u = new URL('https://example.com/path?q=1');\
             document.getElementById('result').textContent = u.hostname + ',' + u.pathname;\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">example.com,/path<"), "got: {}", html);
    }

    #[test]
    fn test_e2e_get_bounding_client_rect() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"target\">box</div><div id=\"result\"></div><script>\
             var r = document.getElementById('target').getBoundingClientRect();\
             document.getElementById('result').textContent = typeof r.width + ',' + typeof r.height;\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">number,number<"), "got: {}", html);
    }

    #[test]
    fn test_e2e_mutation_observer_no_crash() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"result\">ok</div><script>\
             var obs = new MutationObserver(function() {});\
             obs.observe(document.body, { childList: true });\
             obs.disconnect();\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">ok<"), "got: {}", html);
    }

    #[test]
    fn test_e2e_dataset() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"target\" data-user-id=\"42\"></div><div id=\"result\"></div><script>\
             document.getElementById('result').textContent = document.getElementById('target').dataset.userId;\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None, &crate::net::fetch::FetchContext::new(None)).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains(">42<"), "got: {}", html);
    }
