use super::arena::{Arena, NodeId};
use super::node::NodeData;

/// Void elements that must not have a closing tag.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input",
    "link", "meta", "param", "source", "track", "wbr",
];

/// Raw text elements whose children should not be entity-escaped.
const RAW_TEXT_ELEMENTS: &[&str] = &["script", "style"];

/// Serialize the entire document arena to an HTML string.
pub fn serialize_document(arena: &Arena) -> String {
    let mut output = String::new();
    serialize_node(arena, arena.document, &mut output);
    output
}

/// Serialize a single node (and its children) to an HTML string.
/// Used by innerHTML/outerHTML getters in JS bindings.
pub fn serialize_node_to_string(arena: &Arena, id: NodeId, output: &mut String) {
    serialize_node(arena, id, output);
}

fn serialize_node(arena: &Arena, id: NodeId, output: &mut String) {
    let node = &arena.nodes[id];

    match &node.data {
        NodeData::Document => {
            // Serialize children only
            for child in arena.children(id) {
                serialize_node(arena, child, output);
            }
        }
        NodeData::Doctype { name, .. } => {
            output.push_str("<!DOCTYPE ");
            output.push_str(name);
            output.push('>');
        }
        NodeData::Element(data) => {
            let tag = &*data.name.local;
            output.push('<');
            output.push_str(tag);

            for attr in &data.attrs {
                output.push(' ');
                // Handle prefixed attributes
                if let Some(ref prefix) = attr.name.prefix {
                    output.push_str(prefix);
                    output.push(':');
                }
                output.push_str(&attr.name.local);
                output.push_str("=\"");
                escape_attribute(&attr.value, output);
                output.push('"');
            }
            output.push('>');

            let is_void = VOID_ELEMENTS.contains(&tag);
            if !is_void {
                let is_raw = RAW_TEXT_ELEMENTS.contains(&tag);
                for child in arena.children(id) {
                    if is_raw {
                        serialize_raw_child(arena, child, output);
                    } else {
                        serialize_node(arena, child, output);
                    }
                }
                output.push_str("</");
                output.push_str(tag);
                output.push('>');
            }
        }
        NodeData::Text(text) => {
            escape_text(text, output);
        }
        NodeData::Comment(text) => {
            output.push_str("<!--");
            output.push_str(text);
            output.push_str("-->");
        }
    }
}

/// Serialize a child of a raw text element (no escaping).
fn serialize_raw_child(arena: &Arena, id: NodeId, output: &mut String) {
    match &arena.nodes[id].data {
        NodeData::Text(text) => output.push_str(text),
        _ => serialize_node(arena, id, output),
    }
}

fn escape_text(input: &str, output: &mut String) {
    for c in input.chars() {
        match c {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            _ => output.push(c),
        }
    }
}

fn escape_attribute(input: &str, output: &mut String) {
    for c in input.chars() {
        match c {
            '&' => output.push_str("&amp;"),
            '"' => output.push_str("&quot;"),
            _ => output.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::arena::Arena;
    use crate::dom::node::ElementData;
    use crate::dom::treesink;
    use markup5ever::{ns, Attribute, QualName};

    // ─── Parsing-based serialization tests ──────────────────

    #[test]
    fn serialize_simple() {
        let arena = treesink::parse("<p>Hello</p>");
        let html = serialize_document(&arena);
        assert!(html.contains("<p>Hello</p>"), "got: {html}");
    }

    #[test]
    fn serialize_void_elements_no_close_tag() {
        // Only test void elements valid outside table/colgroup context.
        // "col" and "param" get special handling by html5ever's tree builder.
        let testable = &[
            "area", "base", "br", "embed", "hr", "img", "input",
            "link", "meta", "source", "track", "wbr",
        ];
        for tag in testable {
            let input = format!("<body><{tag}></body>");
            let arena = treesink::parse(&input);
            let html = serialize_document(&arena);
            assert!(html.contains(&format!("<{tag}>")), "missing <{tag}> in: {html}");
            assert!(!html.contains(&format!("</{tag}>")), "</{tag}> found in: {html}");
        }
    }

    #[test]
    fn serialize_text_entity_escaping() {
        let arena = treesink::parse("<p>1 &amp; 2 &lt; 3 &gt; 0</p>");
        let html = serialize_document(&arena);
        assert!(html.contains("1 &amp; 2 &lt; 3 &gt; 0"), "got: {html}");
    }

    #[test]
    fn serialize_attribute_entity_escaping() {
        let arena = treesink::parse("<a title=\"say &quot;hello&quot;\">link</a>");
        let html = serialize_document(&arena);
        assert!(html.contains("&quot;"), "attribute quotes should be escaped. got: {html}");
    }

    #[test]
    fn serialize_ampersand_in_attribute() {
        let arena = treesink::parse("<a href=\"?a=1&amp;b=2\">link</a>");
        let html = serialize_document(&arena);
        assert!(html.contains("?a=1&amp;b=2"), "& in attribute should be escaped. got: {html}");
    }

    #[test]
    fn serialize_script_raw_text() {
        let arena = treesink::parse("<script>if (a < b && c > d) {}</script>");
        let html = serialize_document(&arena);
        assert!(
            html.contains("if (a < b && c > d) {}"),
            "script body must NOT be escaped. got: {html}"
        );
        assert!(
            !html.contains("&lt;") || !html.contains("&amp;"),
            "found escaped entities in script. got: {html}"
        );
    }

    #[test]
    fn serialize_style_raw_text() {
        let arena = treesink::parse("<style>.a > .b { color: red; }</style>");
        let html = serialize_document(&arena);
        assert!(
            html.contains(".a > .b { color: red; }"),
            "style body must NOT be escaped. got: {html}"
        );
    }

    #[test]
    fn serialize_comment() {
        let arena = treesink::parse("<!-- my comment --><p>text</p>");
        let html = serialize_document(&arena);
        assert!(html.contains("<!-- my comment -->"), "got: {html}");
    }

    #[test]
    fn serialize_doctype() {
        let arena = treesink::parse("<!DOCTYPE html><html><body></body></html>");
        let html = serialize_document(&arena);
        assert!(html.starts_with("<!DOCTYPE html>"), "got: {html}");
    }

    #[test]
    fn serialize_preserves_attribute_order() {
        let arena = treesink::parse("<div id=\"a\" class=\"b\" data-x=\"c\"></div>");
        let html = serialize_document(&arena);
        let id_pos = html.find("id=").unwrap();
        let class_pos = html.find("class=").unwrap();
        let data_pos = html.find("data-x=").unwrap();
        assert!(id_pos < class_pos, "attribute order not preserved. got: {html}");
        assert!(class_pos < data_pos, "attribute order not preserved. got: {html}");
    }

    #[test]
    fn serialize_nested_structure() {
        let arena = treesink::parse("<ul><li>a</li><li>b</li></ul>");
        let html = serialize_document(&arena);
        assert!(html.contains("<ul><li>a</li><li>b</li></ul>"), "got: {html}");
    }

    #[test]
    fn serialize_empty_elements() {
        let arena = treesink::parse("<div></div>");
        let html = serialize_document(&arena);
        assert!(html.contains("<div></div>"), "got: {html}");
    }

    // ─── Manual arena construction tests ────────────────────
    // (tests the serializer independent of the parser)

    fn build_element(arena: &mut Arena, tag: &str) -> NodeId {
        let name = QualName::new(None, ns!(html), tag.into());
        arena.new_node(NodeData::Element(ElementData::new(name, vec![])))
    }

    fn build_element_with_attrs(arena: &mut Arena, tag: &str, attrs: &[(&str, &str)]) -> NodeId {
        let name = QualName::new(None, ns!(html), tag.into());
        let attrs: Vec<Attribute> = attrs
            .iter()
            .map(|(k, v)| Attribute {
                name: QualName::new(None, ns!(), (*k).into()),
                value: (*v).into(),
            })
            .collect();
        arena.new_node(NodeData::Element(ElementData::new(name, attrs)))
    }

    fn build_text(arena: &mut Arena, text: &str) -> NodeId {
        arena.new_node(NodeData::Text(text.to_string()))
    }

    fn build_comment(arena: &mut Arena, text: &str) -> NodeId {
        arena.new_node(NodeData::Comment(text.to_string()))
    }

    #[test]
    fn serialize_manually_built_tree() {
        let mut arena = Arena::new();
        let html = build_element(&mut arena, "html");
        let body = build_element(&mut arena, "body");
        let p = build_element(&mut arena, "p");
        let text = build_text(&mut arena, "Hello");

        arena.append_child(arena.document, html);
        arena.append_child(html, body);
        arena.append_child(body, p);
        arena.append_child(p, text);

        let output = serialize_document(&arena);
        assert_eq!(output, "<html><body><p>Hello</p></body></html>");
    }

    #[test]
    fn serialize_manually_built_with_attrs() {
        let mut arena = Arena::new();
        let html = build_element(&mut arena, "html");
        let body = build_element(&mut arena, "body");
        let div = build_element_with_attrs(&mut arena, "div", &[("id", "main"), ("class", "container")]);
        let text = build_text(&mut arena, "content");

        arena.append_child(arena.document, html);
        arena.append_child(html, body);
        arena.append_child(body, div);
        arena.append_child(div, text);

        let output = serialize_document(&arena);
        assert!(output.contains("<div id=\"main\" class=\"container\">content</div>"), "got: {output}");
    }

    #[test]
    fn serialize_manually_built_with_comment() {
        let mut arena = Arena::new();
        let html = build_element(&mut arena, "html");
        let body = build_element(&mut arena, "body");
        let comment = build_comment(&mut arena, " TODO ");
        let p = build_element(&mut arena, "p");

        arena.append_child(arena.document, html);
        arena.append_child(html, body);
        arena.append_child(body, comment);
        arena.append_child(body, p);

        let output = serialize_document(&arena);
        assert!(output.contains("<!-- TODO -->"), "got: {output}");
    }

    #[test]
    fn serialize_text_with_special_chars() {
        let mut arena = Arena::new();
        let html = build_element(&mut arena, "html");
        let body = build_element(&mut arena, "body");
        let p = build_element(&mut arena, "p");
        let text = build_text(&mut arena, "a < b & c > d");

        arena.append_child(arena.document, html);
        arena.append_child(html, body);
        arena.append_child(body, p);
        arena.append_child(p, text);

        let output = serialize_document(&arena);
        assert!(output.contains("a &lt; b &amp; c &gt; d"), "got: {output}");
    }

    #[test]
    fn serialize_attribute_with_quotes() {
        let mut arena = Arena::new();
        let html = build_element(&mut arena, "html");
        let body = build_element(&mut arena, "body");
        let div = build_element_with_attrs(&mut arena, "div", &[("title", "say \"hi\"")]);

        arena.append_child(arena.document, html);
        arena.append_child(html, body);
        arena.append_child(body, div);

        let output = serialize_document(&arena);
        assert!(output.contains("title=\"say &quot;hi&quot;\""), "got: {output}");
    }

    #[test]
    fn serialize_void_element_never_gets_close_tag() {
        let mut arena = Arena::new();
        let html = build_element(&mut arena, "html");
        let body = build_element(&mut arena, "body");
        let br = build_element(&mut arena, "br");
        let img = build_element_with_attrs(&mut arena, "img", &[("src", "pic.png")]);

        arena.append_child(arena.document, html);
        arena.append_child(html, body);
        arena.append_child(body, br);
        arena.append_child(body, img);

        let output = serialize_document(&arena);
        assert!(output.contains("<br>"), "got: {output}");
        assert!(!output.contains("</br>"), "got: {output}");
        assert!(output.contains("<img src=\"pic.png\">"), "got: {output}");
        assert!(!output.contains("</img>"), "got: {output}");
    }

    #[test]
    fn serialize_script_element_raw() {
        let mut arena = Arena::new();
        let html = build_element(&mut arena, "html");
        let body = build_element(&mut arena, "body");
        let script = build_element(&mut arena, "script");
        let code = build_text(&mut arena, "var x = 1 < 2 && 3 > 0;");

        arena.append_child(arena.document, html);
        arena.append_child(html, body);
        arena.append_child(body, script);
        arena.append_child(script, code);

        let output = serialize_document(&arena);
        assert!(
            output.contains("<script>var x = 1 < 2 && 3 > 0;</script>"),
            "script text must be raw. got: {output}"
        );
    }

    #[test]
    fn serialize_empty_document() {
        let arena = Arena::new();
        let output = serialize_document(&arena);
        // Document with no children should produce empty string
        assert_eq!(output, "");
    }

    #[test]
    fn serialize_doctype_node() {
        let mut arena = Arena::new();
        let doctype = arena.new_node(NodeData::Doctype {
            name: "html".to_string(),
            public_id: String::new(),
            system_id: String::new(),
        });
        let html = build_element(&mut arena, "html");

        arena.append_child(arena.document, doctype);
        arena.append_child(arena.document, html);

        let output = serialize_document(&arena);
        assert!(output.starts_with("<!DOCTYPE html>"), "got: {output}");
    }
}
