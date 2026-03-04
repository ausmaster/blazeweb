    use super::*;
    use crate::dom::treesink;

    #[test]
    fn test_query_selector_by_tag() {
        let arena = treesink::parse("<html><body><div><p>Hello</p><span>World</span></div></body></html>");
        let result = query_selector(&arena, arena.document, "p").unwrap();
        assert!(result.is_some());
        let id = result.unwrap();
        assert_eq!(arena.element_data(id).unwrap().name.local.as_ref(), "p");
    }

    #[test]
    fn test_query_selector_by_id() {
        let arena = treesink::parse("<html><body><div id=\"target\">Hit</div></body></html>");
        let result = query_selector(&arena, arena.document, "#target").unwrap();
        let id = result.expect("should find #target");
        assert_eq!(arena.element_data(id).unwrap().get_attribute("id"), Some("target"));
        assert_eq!(arena.element_data(id).unwrap().name.local.as_ref(), "div");
    }

    #[test]
    fn test_query_selector_by_class() {
        let arena = treesink::parse("<html><body><div class=\"foo bar\">Hit</div><div class=\"baz\">Miss</div></body></html>");
        let result = query_selector(&arena, arena.document, ".foo").unwrap();
        assert!(result.is_some());
        let id = result.unwrap();
        assert_eq!(arena.element_data(id).unwrap().get_attribute("class"), Some("foo bar"));
    }

    #[test]
    fn test_query_selector_all() {
        let arena = treesink::parse("<html><body><p>A</p><p>B</p><p>C</p></body></html>");
        let results = query_selector_all(&arena, arena.document, "p").unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_query_selector_compound() {
        let arena = treesink::parse("<html><body><div class=\"a\"><span class=\"b\">Hit</span></div></body></html>");
        let result = query_selector(&arena, arena.document, "div.a span.b").unwrap();
        let id = result.expect("should find div.a span.b");
        assert_eq!(arena.element_data(id).unwrap().name.local.as_ref(), "span");
        assert_eq!(arena.element_data(id).unwrap().get_attribute("class"), Some("b"));
    }

    #[test]
    fn test_matches_element() {
        let arena = treesink::parse("<html><body><div id=\"x\" class=\"y\">Z</div></body></html>");
        let div = arena.find_element(arena.document, "div").unwrap();
        assert!(matches_element(&arena, div, "div#x.y").unwrap());
        assert!(!matches_element(&arena, div, "span").unwrap());
    }

    #[test]
    fn test_closest() {
        let arena = treesink::parse("<html><body><div class=\"outer\"><p class=\"inner\">Text</p></div></body></html>");
        let p = arena.find_element(arena.document, "p").unwrap();
        let result = closest(&arena, p, ".outer").unwrap();
        assert!(result.is_some());
        let id = result.unwrap();
        assert_eq!(arena.element_data(id).unwrap().name.local.as_ref(), "div");
    }

    #[test]
    fn test_closest_self() {
        let arena = treesink::parse("<html><body><div class=\"target\">Text</div></body></html>");
        let div = arena.find_element(arena.document, "div").unwrap();
        let result = closest(&arena, div, ".target").unwrap();
        assert_eq!(result, Some(div));
    }

    #[test]
    fn test_invalid_selector() {
        let arena = treesink::parse("<html><body></body></html>");
        let result = query_selector(&arena, arena.document, "[[[invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_attribute_selector() {
        let arena = treesink::parse("<html><body><input type=\"text\"><input type=\"hidden\"></body></html>");
        let results = query_selector_all(&arena, arena.document, "input[type=\"text\"]").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_child_combinator() {
        let arena = treesink::parse("<html><body><div><p>Direct</p></div><section><div><p>Nested</p></div></section></body></html>");
        let results = query_selector_all(&arena, arena.document, "body > div > p").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_nth_child() {
        let arena = treesink::parse("<html><body><ul><li>1</li><li>2</li><li>3</li></ul></body></html>");
        let result = query_selector(&arena, arena.document, "li:nth-child(2)").unwrap();
        let id = result.expect("should find li:nth-child(2)");
        assert_eq!(arena.element_data(id).unwrap().name.local.as_ref(), "li");
        // Verify it's the second li by checking its text child is "2"
        let children: Vec<_> = arena.children(id).collect();
        assert!(!children.is_empty());
        match &arena.nodes[children[0]].data {
            crate::dom::node::NodeData::Text(t) => assert_eq!(t, "2"),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn test_not_pseudo_class() {
        let arena = treesink::parse("<html><body><div class=\"a\">A</div><div class=\"b\">B</div></body></html>");
        let results = query_selector_all(&arena, arena.document, "div:not(.a)").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_empty_result() {
        let arena = treesink::parse("<html><body><div>Text</div></body></html>");
        let result = query_selector(&arena, arena.document, "span.nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_scoped_query() {
        let arena = treesink::parse("<html><body><div id=\"a\"><p>In A</p></div><div id=\"b\"><p>In B</p></div></body></html>");
        let div_a = query_selector(&arena, arena.document, "#a").unwrap().unwrap();
        let results = query_selector_all(&arena, div_a, "p").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_universal_selector() {
        let arena = treesink::parse("<html><body><div><p>A</p><span>B</span></div></body></html>");
        let div = arena.find_element(arena.document, "div").unwrap();
        let results = query_selector_all(&arena, div, "*").unwrap();
        assert_eq!(results.len(), 2); // p and span
    }

    #[test]
    fn test_multiple_selectors() {
        let arena = treesink::parse("<html><body><p>P</p><span>S</span><div>D</div></body></html>");
        let results = query_selector_all(&arena, arena.document, "p, span").unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_attribute_prefix_selector() {
        let arena = treesink::parse(r#"<html><body><div data-x="foobar"></div><div data-x="bazqux"></div></body></html>"#);
        let results = query_selector_all(&arena, arena.document, "[data-x^=\"foo\"]").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_attribute_suffix_selector() {
        let arena = treesink::parse(r#"<html><body><div data-x="foobar"></div><div data-x="bazbar"></div><div data-x="nope"></div></body></html>"#);
        let results = query_selector_all(&arena, arena.document, "[data-x$=\"bar\"]").unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_attribute_contains_selector() {
        let arena = treesink::parse(r#"<html><body><div data-x="hello world"></div><div data-x="nope"></div></body></html>"#);
        let results = query_selector_all(&arena, arena.document, "[data-x*=\"lo wo\"]").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_adjacent_sibling_combinator() {
        let arena = treesink::parse("<html><body><p>A</p><div>B</div><span>C</span></body></html>");
        let results = query_selector_all(&arena, arena.document, "p + div").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_general_sibling_combinator() {
        let arena = treesink::parse("<html><body><p>A</p><div>B</div><span>C</span></body></html>");
        let results = query_selector_all(&arena, arena.document, "p ~ span").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_first_child_pseudo() {
        let arena = treesink::parse("<html><body><ul><li>1</li><li>2</li><li>3</li></ul></body></html>");
        let results = query_selector_all(&arena, arena.document, "li:first-child").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_last_child_pseudo() {
        let arena = treesink::parse("<html><body><ul><li>1</li><li>2</li><li>3</li></ul></body></html>");
        let results = query_selector_all(&arena, arena.document, "li:last-child").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_nth_child_odd() {
        let arena = treesink::parse("<html><body><ul><li>1</li><li>2</li><li>3</li><li>4</li></ul></body></html>");
        let results = query_selector_all(&arena, arena.document, "li:nth-child(odd)").unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_case_insensitive_attr() {
        let arena = treesink::parse(r#"<html><body><div data-x="FOO"></div></body></html>"#);
        let results = query_selector_all(&arena, arena.document, "[data-x=\"FOO\"]").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_closest_no_match() {
        let arena = treesink::parse("<html><body><div><p>Text</p></div></body></html>");
        let p = arena.find_element(arena.document, "p").unwrap();
        let result = closest(&arena, p, ".nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_matches_on_non_element() {
        let arena = treesink::parse("<html><body>Text</body></html>");
        // document node is not an element
        assert!(!matches_element(&arena, arena.document, "html").unwrap());
    }

    #[test]
    fn test_deeply_nested() {
        let arena = treesink::parse(
            "<html><body><div><div><div><div><span id=\"deep\">X</span></div></div></div></div></body></html>"
        );
        let result = query_selector(&arena, arena.document, "#deep").unwrap();
        let id = result.expect("should find #deep in nested tree");
        assert_eq!(arena.element_data(id).unwrap().name.local.as_ref(), "span");
        assert_eq!(arena.element_data(id).unwrap().get_attribute("id"), Some("deep"));
    }
