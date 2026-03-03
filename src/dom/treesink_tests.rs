    use super::*;
    use crate::dom::node::NodeData;
    use crate::dom::serialize::serialize_document;

    /// Helper: collect children as NodeIds.
    fn child_vec(arena: &Arena, parent: NodeId) -> Vec<NodeId> {
        arena.children(parent).collect()
    }

    /// Helper: get the local tag name of an element node.
    fn tag_name(arena: &Arena, id: NodeId) -> &str {
        match &arena.nodes[id].data {
            NodeData::Element(data) => &data.name.local,
            other => panic!("expected Element, got {other:?}"),
        }
    }

    /// Helper: get text content of a text node.
    fn text_content(arena: &Arena, id: NodeId) -> &str {
        match &arena.nodes[id].data {
            NodeData::Text(s) => s,
            other => panic!("expected Text, got {other:?}"),
        }
    }

    // ─── Basic parsing ──────────────────────────────────────

    #[test]
    fn parse_minimal_document() {
        let arena = parse("<html><head></head><body></body></html>");
        let doc_children = child_vec(&arena, arena.document);
        assert_eq!(doc_children.len(), 1, "document should have one child (html)");

        let html = doc_children[0];
        assert_eq!(tag_name(&arena, html), "html");

        let html_children = child_vec(&arena, html);
        assert_eq!(html_children.len(), 2, "html should have head and body");
        assert_eq!(tag_name(&arena, html_children[0]), "head");
        assert_eq!(tag_name(&arena, html_children[1]), "body");
    }

    #[test]
    fn parse_creates_implied_elements() {
        // html5ever should create <html>, <head>, <body> even from a bare fragment
        let arena = parse("<p>Hello</p>");
        let html = arena.find_element(arena.document, "html");
        assert!(html.is_some(), "should create implied <html>");
        let body = arena.find_element(arena.document, "body");
        assert!(body.is_some(), "should create implied <body>");
        let head = arena.find_element(arena.document, "head");
        assert!(head.is_some(), "should create implied <head>");
    }

    #[test]
    fn parse_text_content() {
        let arena = parse("<p>Hello World</p>");
        let p = arena.find_element(arena.document, "p").unwrap();
        let p_children = child_vec(&arena, p);
        assert_eq!(p_children.len(), 1);
        assert_eq!(text_content(&arena, p_children[0]), "Hello World");
    }

    #[test]
    fn parse_nested_elements() {
        let arena = parse("<div><span><a href=\"#\">link</a></span></div>");
        let div = arena.find_element(arena.document, "div").unwrap();
        let span = arena.find_element(arena.document, "span").unwrap();
        let a = arena.find_element(arena.document, "a").unwrap();

        assert_eq!(arena.nodes[span].parent, Some(div));
        assert_eq!(arena.nodes[a].parent, Some(span));

        let a_data = arena.element_data(a).unwrap();
        assert_eq!(a_data.get_attribute("href"), Some("#"));
    }

    // ─── Attributes ─────────────────────────────────────────

    #[test]
    fn parse_preserves_attributes() {
        let arena = parse("<div id=\"main\" class=\"container\" data-x=\"42\"></div>");
        let div = arena.find_element(arena.document, "div").unwrap();
        let data = arena.element_data(div).unwrap();
        assert_eq!(data.get_attribute("id"), Some("main"));
        assert_eq!(data.get_attribute("class"), Some("container"));
        assert_eq!(data.get_attribute("data-x"), Some("42"));
    }

    #[test]
    fn parse_boolean_attributes() {
        let arena = parse("<input disabled readonly>");
        let input = arena.find_element(arena.document, "input").unwrap();
        let data = arena.element_data(input).unwrap();
        // html5ever represents boolean attributes with empty string value per spec
        assert_eq!(data.get_attribute("disabled"), Some(""));
        assert_eq!(data.get_attribute("readonly"), Some(""));
    }

    #[test]
    fn parse_attribute_with_special_chars() {
        let arena = parse("<a href=\"/path?a=1&amp;b=2\">link</a>");
        let a = arena.find_element(arena.document, "a").unwrap();
        let data = arena.element_data(a).unwrap();
        // html5ever decodes entities in attributes
        assert_eq!(data.get_attribute("href"), Some("/path?a=1&b=2"));
    }

    // ─── Multiple script tags ───────────────────────────────

    #[test]
    fn parse_multiple_scripts() {
        let html = "<html><body>\
            <script>var a = 1;</script>\
            <script>var b = 2;</script>\
            <script src=\"ext.js\"></script>\
        </body></html>";
        let arena = parse(html);
        let body = arena.find_element(arena.document, "body").unwrap();

        let scripts: Vec<_> = arena
            .children(body)
            .filter(|&id| {
                arena
                    .element_data(id)
                    .is_some_and(|d| &*d.name.local == "script")
            })
            .collect();

        assert_eq!(scripts.len(), 3, "should find 3 script elements");

        // First two have text content, third has src attribute
        let s1_children = child_vec(&arena, scripts[0]);
        assert_eq!(s1_children.len(), 1);
        assert_eq!(text_content(&arena, s1_children[0]), "var a = 1;");

        let s3_data = arena.element_data(scripts[2]).unwrap();
        assert_eq!(s3_data.get_attribute("src"), Some("ext.js"));
    }

    // ─── Doctype ────────────────────────────────────────────

    #[test]
    fn parse_doctype() {
        let arena = parse("<!DOCTYPE html><html><body></body></html>");
        let doc_children = child_vec(&arena, arena.document);
        assert!(doc_children.len() >= 2, "should have doctype + html");

        match &arena.nodes[doc_children[0]].data {
            NodeData::Doctype { name, .. } => assert_eq!(name, "html"),
            other => panic!("expected Doctype, got {other:?}"),
        }
    }

    // ─── Comments ───────────────────────────────────────────

    #[test]
    fn parse_comments() {
        let arena = parse("<body><!-- hello --><p>text</p><!-- world --></body>");
        let body = arena.find_element(arena.document, "body").unwrap();
        let children = child_vec(&arena, body);

        // Should have: comment, p, comment
        assert_eq!(children.len(), 3, "body children: {children:?}");
        assert!(matches!(&arena.nodes[children[0]].data, NodeData::Comment(s) if s.contains("hello")));
        assert_eq!(tag_name(&arena, children[1]), "p");
        assert!(matches!(&arena.nodes[children[2]].data, NodeData::Comment(s) if s.contains("world")));
    }

    // ─── Text merging ───────────────────────────────────────

    #[test]
    fn parse_merges_adjacent_text() {
        // html5ever should merge consecutive text nodes within the same parent.
        // (This happens via the TreeSink::append optimization.)
        let arena = parse("<p>Hello World</p>");
        let p = arena.find_element(arena.document, "p").unwrap();
        let children = child_vec(&arena, p);
        assert_eq!(children.len(), 1, "adjacent text should merge into one node");
    }

    #[test]
    fn parse_mixed_inline_text() {
        let arena = parse("<p>before <b>bold</b> after</p>");
        let p = arena.find_element(arena.document, "p").unwrap();
        let children = child_vec(&arena, p);
        // Should be: Text("before "), <b>, Text(" after")
        assert_eq!(children.len(), 3);
        assert_eq!(text_content(&arena, children[0]), "before ");
        assert_eq!(tag_name(&arena, children[1]), "b");
        assert_eq!(text_content(&arena, children[2]), " after");
    }

    // ─── Error recovery (malformed HTML) ────────────────────

    #[test]
    fn parse_unclosed_tags() {
        let arena = parse("<div><p>paragraph<span>span");
        // html5ever auto-closes unclosed tags. Expected tree:
        // <div> → <p> → "paragraph" + <span> → "span"
        let div = arena.find_element(arena.document, "div").unwrap();
        let div_children = child_vec(&arena, div);
        assert_eq!(div_children.len(), 1, "div should have 1 child (p)");
        assert_eq!(tag_name(&arena, div_children[0]), "p");
        let p_children = child_vec(&arena, div_children[0]);
        assert_eq!(p_children.len(), 2, "p should have text + span");
        assert_eq!(text_content(&arena, p_children[0]), "paragraph");
        assert_eq!(tag_name(&arena, p_children[1]), "span");
    }

    #[test]
    fn parse_overlapping_tags() {
        // <b> and <i> overlap — html5ever uses the adoption agency algorithm.
        // Expected tree per spec: <p> → <b>"bold "<i>"both"</i></b> + <i>" italic"</i>
        let arena = parse("<p><b>bold <i>both</b> italic</i></p>");
        let p = arena.find_element(arena.document, "p").unwrap();
        let p_children = child_vec(&arena, p);
        assert_eq!(p_children.len(), 2, "p should have <b> and <i>");
        assert_eq!(tag_name(&arena, p_children[0]), "b");
        assert_eq!(tag_name(&arena, p_children[1]), "i");
        // Check <b> contains "bold " text and nested <i>"both"
        let b_children = child_vec(&arena, p_children[0]);
        assert_eq!(b_children.len(), 2, "b should have text + i");
        assert_eq!(text_content(&arena, b_children[0]), "bold ");
        assert_eq!(tag_name(&arena, b_children[1]), "i");
    }

    #[test]
    fn parse_table_foster_parenting() {
        // Text directly inside <table> triggers foster parenting in html5.
        // "oops" gets foster-parented before the table.
        let arena = parse("<table>oops<tr><td>cell</td></tr></table>");
        let body = arena.find_element(arena.document, "body").unwrap();
        let body_children = child_vec(&arena, body);
        assert_eq!(body_children.len(), 2, "body should have text + table");
        assert_eq!(text_content(&arena, body_children[0]), "oops");
        assert_eq!(tag_name(&arena, body_children[1]), "table");
    }

    #[test]
    fn parse_empty_string() {
        let arena = parse("");
        // html5ever should still produce a valid document with implied html/head/body
        assert!(arena.find_element(arena.document, "html").is_some());
    }

    // ─── Void elements ──────────────────────────────────────

    #[test]
    fn parse_void_elements_have_no_children() {
        let arena = parse("<br><hr><img src=\"x.png\"><input type=\"text\">");
        let br = arena.find_element(arena.document, "br").unwrap();
        let hr = arena.find_element(arena.document, "hr").unwrap();
        let img = arena.find_element(arena.document, "img").unwrap();
        let input = arena.find_element(arena.document, "input").unwrap();

        assert_eq!(child_vec(&arena, br), vec![]);
        assert_eq!(child_vec(&arena, hr), vec![]);
        assert_eq!(child_vec(&arena, img), vec![]);
        assert_eq!(child_vec(&arena, input), vec![]);
    }

    // ─── Parse → serialize roundtrip ────────────────────────

    #[test]
    fn roundtrip_simple() {
        let input = "<html><head></head><body><p>Hello</p></body></html>";
        let arena = parse(input);
        let output = serialize_document(&arena);
        assert!(output.contains("<p>Hello</p>"), "got: {output}");
        assert!(output.contains("<body>"), "got: {output}");
        assert!(output.contains("</html>"), "got: {output}");
    }

    #[test]
    fn roundtrip_attributes() {
        let arena = parse("<div id=\"main\" class=\"foo bar\"></div>");
        let output = serialize_document(&arena);
        assert!(output.contains("id=\"main\""), "got: {output}");
        assert!(output.contains("class=\"foo bar\""), "got: {output}");
    }

    #[test]
    fn roundtrip_entities_in_text() {
        let arena = parse("<p>1 &lt; 2 &amp; 3 &gt; 0</p>");
        let output = serialize_document(&arena);
        assert!(output.contains("1 &lt; 2 &amp; 3 &gt; 0"), "got: {output}");
    }

    #[test]
    fn roundtrip_entities_in_attributes() {
        let arena = parse("<a href=\"/path?a=1&amp;b=2\">link</a>");
        let output = serialize_document(&arena);
        // Attribute should re-escape the &
        assert!(output.contains("href=\"/path?a=1&amp;b=2\""), "got: {output}");
    }

    #[test]
    fn roundtrip_script_not_escaped() {
        let arena = parse("<script>if (a < b && c > d) { alert('hi'); }</script>");
        let output = serialize_document(&arena);
        assert!(
            output.contains("if (a < b && c > d) { alert('hi'); }"),
            "script body should not be entity-escaped. got: {output}"
        );
    }

    #[test]
    fn roundtrip_style_not_escaped() {
        let arena = parse("<style>.foo > .bar { color: red; }</style>");
        let output = serialize_document(&arena);
        assert!(
            output.contains(".foo > .bar { color: red; }"),
            "style body should not be entity-escaped. got: {output}"
        );
    }

    #[test]
    fn roundtrip_void_elements() {
        let arena = parse("<br><hr><img src=\"x.png\">");
        let output = serialize_document(&arena);
        assert!(output.contains("<br>"), "got: {output}");
        assert!(output.contains("<hr>"), "got: {output}");
        assert!(output.contains("<img src=\"x.png\">"), "got: {output}");
        assert!(!output.contains("</br>"), "void elements must not have closing tags. got: {output}");
        assert!(!output.contains("</img>"), "void elements must not have closing tags. got: {output}");
    }

    #[test]
    fn roundtrip_comment() {
        let arena = parse("<!-- test comment --><p>text</p>");
        let output = serialize_document(&arena);
        assert!(output.contains("<!-- test comment -->"), "got: {output}");
    }

    #[test]
    fn roundtrip_doctype() {
        let arena = parse("<!DOCTYPE html><html><body></body></html>");
        let output = serialize_document(&arena);
        assert!(output.contains("<!DOCTYPE html>"), "got: {output}");
    }

    #[test]
    fn roundtrip_deeply_nested() {
        let arena = parse("<div><div><div><div><p>deep</p></div></div></div></div>");
        let output = serialize_document(&arena);
        assert!(output.contains("<div><div><div><div><p>deep</p></div></div></div></div>"), "got: {output}");
    }

    #[test]
    fn roundtrip_empty_elements() {
        let arena = parse("<div></div><span></span><p></p>");
        let output = serialize_document(&arena);
        assert!(output.contains("<div></div>"), "got: {output}");
        assert!(output.contains("<span></span>"), "got: {output}");
        assert!(output.contains("<p></p>"), "got: {output}");
    }

    #[test]
    fn roundtrip_multiple_attributes() {
        let arena = parse("<input type=\"text\" name=\"field\" value=\"hello\" placeholder=\"enter\">");
        let output = serialize_document(&arena);
        assert!(output.contains("type=\"text\""), "got: {output}");
        assert!(output.contains("name=\"field\""), "got: {output}");
        assert!(output.contains("value=\"hello\""), "got: {output}");
    }

    // ─── Real-world-ish HTML ────────────────────────────────

    #[test]
    fn parse_realistic_page() {
        let html = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <title>Test Page</title>
    <link rel="stylesheet" href="style.css">
</head>
<body>
    <header>
        <nav><a href="/">Home</a> | <a href="/about">About</a></nav>
    </header>
    <main>
        <h1>Welcome</h1>
        <p>This is a <strong>test</strong> page with <em>formatting</em>.</p>
        <ul>
            <li>Item 1</li>
            <li>Item 2</li>
            <li>Item 3</li>
        </ul>
    </main>
    <script>
        document.querySelector('h1').textContent = 'Modified';
    </script>
</body>
</html>"#;

        let arena = parse(html);
        let output = serialize_document(&arena);

        // Structure should be preserved
        assert!(output.contains("<title>Test Page</title>"), "got: {output}");
        assert!(output.contains("<h1>Welcome</h1>"), "got: {output}");
        assert!(output.contains("<strong>test</strong>"), "got: {output}");
        assert!(output.contains("<li>Item 1</li>"), "got: {output}");
        assert!(output.contains("lang=\"en\""), "got: {output}");

        // Script should be raw (not escaped)
        assert!(output.contains("document.querySelector('h1')"), "got: {output}");

        // Structural integrity
        assert!(arena.find_element(arena.document, "header").is_some());
        assert!(arena.find_element(arena.document, "nav").is_some());
        assert!(arena.find_element(arena.document, "main").is_some());

        // Count li elements
        let ul = arena.find_element(arena.document, "ul").unwrap();
        let lis: Vec<_> = arena
            .children(ul)
            .filter(|&id| arena.element_data(id).is_some_and(|d| &*d.name.local == "li"))
            .collect();
        assert_eq!(lis.len(), 3);
    }

    #[test]
    fn selectedcontent_clones_first_option() {
        // Single option (implicitly closed) — should be cloned into selectedcontent
        let arena = parse("<select><button><selectedcontent></button><option>X");
        let select = arena.find_element(arena.document, "select").unwrap();
        let sc = arena.find_element(select, "selectedcontent").unwrap();
        let sc_children: Vec<_> = arena.children(sc).collect();
        assert_eq!(sc_children.len(), 1, "selectedcontent should have 1 cloned child");
        assert_eq!(text_content(&arena, sc_children[0]), "X");
    }

    #[test]
    fn selectedcontent_clones_selected_attr() {
        // Two options, second has `selected` — should clone "Y" not "X"
        let arena = parse(
            "<select><button><selectedcontent></button><option>X<option selected>Y"
        );
        let select = arena.find_element(arena.document, "select").unwrap();
        let sc = arena.find_element(select, "selectedcontent").unwrap();
        let sc_children: Vec<_> = arena.children(sc).collect();
        assert_eq!(sc_children.len(), 1);
        assert_eq!(text_content(&arena, sc_children[0]), "Y");
    }

    #[test]
    fn selectedcontent_first_of_multiple() {
        // Two options, no `selected` — first should be cloned
        let arena = parse(
            "<select><button><selectedcontent></button><option>X<option>Y"
        );
        let select = arena.find_element(arena.document, "select").unwrap();
        let sc = arena.find_element(select, "selectedcontent").unwrap();
        let sc_children: Vec<_> = arena.children(sc).collect();
        assert_eq!(sc_children.len(), 1);
        assert_eq!(text_content(&arena, sc_children[0]), "X");
    }

    #[test]
    fn selectedcontent_no_selectedcontent_element() {
        // No <selectedcontent> — nothing should crash
        let arena = parse("<select><option>X");
        let select = arena.find_element(arena.document, "select").unwrap();
        assert!(arena.find_element(select, "selectedcontent").is_none());
    }

    #[test]
    fn parse_template_has_contents() {
        let arena = parse("<template><div>inside</div></template>");
        let tmpl = arena.find_element(arena.document, "template").unwrap();
        if let NodeData::Element(data) = &arena.nodes[tmpl].data {
            assert!(
                data.template_contents.is_some(),
                "template element should have template_contents set"
            );
            let contents = data.template_contents.unwrap();
            let children: Vec<_> = arena.children(contents).collect();
            assert!(!children.is_empty(), "template contents should have children");
            assert_eq!(tag_name(&arena, children[0]), "div");
        } else {
            panic!("expected element node for template");
        }
    }
