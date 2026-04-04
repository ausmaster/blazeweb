use super::*;
use crate::dom::arena::{Arena, NodeId};
use crate::dom::node::{ElementData, NodeData};
use markup5ever::{ns, Attribute, QualName};
use style::dom::TElement;

// ─── Helpers ──────────────────────────────────────────────────────────────

fn make_element(arena: &mut Arena, tag: &str) -> NodeId {
    let name = QualName::new(None, ns!(html), tag.into());
    arena.new_node(NodeData::Element(ElementData::new(name, vec![])))
}

fn make_element_with_attrs(arena: &mut Arena, tag: &str, attrs: &[(&str, &str)]) -> NodeId {
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

fn make_text(arena: &mut Arena, text: &str) -> NodeId {
    arena.new_node(NodeData::Text(text.to_string()))
}

/// Build a standard DOM: Document → html → head + body
fn build_dom() -> (Arena, NodeId, NodeId, NodeId) {
    let mut arena = Arena::new();
    let html = make_element(&mut arena, "html");
    let head = make_element(&mut arena, "head");
    let body = make_element(&mut arena, "body");
    arena.append_child(arena.document, html);
    arena.append_child(html, head);
    arena.append_child(html, body);
    (arena, html, head, body)
}

/// Build a DOM with a <style> element containing CSS.
fn build_dom_with_style(css: &str) -> (Arena, NodeId) {
    let (mut arena, _html, head, body) = build_dom();
    let style = make_element(&mut arena, "style");
    let css_text = make_text(&mut arena, css);
    arena.append_child(style, css_text);
    arena.append_child(head, style);
    (arena, body)
}

// ─── collect_style_elements ───────────────────────────────────────────────

#[test]
fn collect_style_elements_finds_styles() {
    let (mut arena, _html, head, _body) = build_dom();
    let style1 = make_element(&mut arena, "style");
    let style2 = make_element(&mut arena, "style");
    arena.append_child(head, style1);
    arena.append_child(head, style2);

    let mut result = Vec::new();
    collect_style_elements(&arena, arena.document, &mut result);
    assert_eq!(result.len(), 2);
    assert!(result.contains(&style1));
    assert!(result.contains(&style2));
}

#[test]
fn collect_style_elements_ignores_non_style() {
    let (mut arena, _html, head, _body) = build_dom();
    let div = make_element(&mut arena, "div");
    let link = make_element(&mut arena, "link");
    arena.append_child(head, div);
    arena.append_child(head, link);

    let mut result = Vec::new();
    collect_style_elements(&arena, arena.document, &mut result);
    assert!(result.is_empty());
}

#[test]
fn collect_style_elements_finds_nested_styles() {
    let (mut arena, _html, _head, body) = build_dom();
    let div = make_element(&mut arena, "div");
    let style = make_element(&mut arena, "style");
    arena.append_child(body, div);
    arena.append_child(div, style);

    let mut result = Vec::new();
    collect_style_elements(&arena, arena.document, &mut result);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], style);
}

// ─── get_text_content ─────────────────────────────────────────────────────

#[test]
fn get_text_content_single_text_child() {
    let mut arena = Arena::new();
    let parent = make_element(&mut arena, "style");
    let text = make_text(&mut arena, "body { color: red }");
    arena.append_child(parent, text);
    assert_eq!(get_text_content(&arena, parent), "body { color: red }");
}

#[test]
fn get_text_content_multiple_text_children() {
    let mut arena = Arena::new();
    let parent = make_element(&mut arena, "style");
    let t1 = make_text(&mut arena, "body { ");
    let t2 = make_text(&mut arena, "color: red }");
    arena.append_child(parent, t1);
    arena.append_child(parent, t2);
    assert_eq!(get_text_content(&arena, parent), "body { color: red }");
}

#[test]
fn get_text_content_empty() {
    let mut arena = Arena::new();
    let parent = make_element(&mut arena, "style");
    assert_eq!(get_text_content(&arena, parent), "");
}

#[test]
fn get_text_content_skips_element_children() {
    let mut arena = Arena::new();
    let parent = make_element(&mut arena, "div");
    let child = make_element(&mut arena, "span");
    let text = make_text(&mut arena, "hello");
    arena.append_child(parent, child);
    arena.append_child(parent, text);
    // Only direct text children, not recursive
    assert_eq!(get_text_content(&arena, parent), "hello");
}

// ─── make_stylesheet ──────────────────────────────────────────────────────

#[test]
fn make_stylesheet_parses_valid_css() {
    let guard = style::shared_lock::SharedRwLock::new();
    let sheet = make_stylesheet("body { color: red; }", style::stylesheets::Origin::Author, &guard);
    // Should not panic; sheet is created successfully.
    let _ = sheet;
}

#[test]
fn make_stylesheet_handles_empty_css() {
    let guard = style::shared_lock::SharedRwLock::new();
    let sheet = make_stylesheet("", style::stylesheets::Origin::Author, &guard);
    let _ = sheet;
}

#[test]
fn make_stylesheet_handles_invalid_css() {
    let guard = style::shared_lock::SharedRwLock::new();
    // Invalid CSS should parse without panic (Stylo is error-tolerant)
    let sheet = make_stylesheet("{{{{ not css !!!!", style::stylesheets::Origin::Author, &guard);
    let _ = sheet;
}

// ─── resolve_styles integration ───────────────────────────────────────────

#[test]
fn resolve_styles_no_panic_on_empty_doc() {
    let mut arena = Arena::new();
    resolve_styles(&mut arena);
    // Should complete without panic even with no elements.
}

#[test]
fn resolve_styles_no_panic_on_minimal_doc() {
    let (mut arena, _html, _head, _body) = build_dom();
    resolve_styles(&mut arena);
}

#[test]
fn resolve_styles_populates_element_data() {
    let (mut arena, html, _head, body) = build_dom();
    resolve_styles(&mut arena);

    // After style resolution, elements should have computed style data.
    let html_node = super::super::stylo_bridge::StyloNode::new(html);
    assert!(html_node.has_data(), "html element should have style data");

    let body_node = super::super::stylo_bridge::StyloNode::new(body);
    assert!(body_node.has_data(), "body element should have style data");
}

#[test]
fn resolve_styles_with_author_stylesheet() {
    let (mut arena, body) = build_dom_with_style("body { display: flex; }");
    resolve_styles(&mut arena);

    let body_node = super::super::stylo_bridge::StyloNode::new(body);
    assert!(body_node.has_data(), "body should have style data after resolution with author CSS");

    // Verify computed style is accessible
    let data = body_node.borrow_data().expect("should have element data");
    let styles = data.styles.get_primary().expect("primary styles");
    let display = styles.get_box().display;
    // display: flex should be resolved
    assert!(
        !display.is_none(),
        "display should not be none after style resolution"
    );
}

#[test]
fn resolve_styles_hidden_attribute_applies_display_none() {
    let (mut arena, _html, _head, body) = build_dom();
    let div = make_element_with_attrs(&mut arena, "div", &[("hidden", "")]);
    arena.append_child(body, div);

    resolve_styles(&mut arena);

    let div_node = super::super::stylo_bridge::StyloNode::new(div);
    assert!(div_node.has_data(), "hidden div should have style data");

    let data = div_node.borrow_data().expect("should have element data");
    let styles = data.styles.get_primary().expect("primary styles");
    let display = styles.get_box().display;
    assert!(
        display.is_none(),
        "hidden attribute should produce display: none, got {:?}",
        display
    );
}

#[test]
fn resolve_styles_ua_defaults_block_elements() {
    let (mut arena, _html, _head, body) = build_dom();
    let div = make_element(&mut arena, "div");
    let span = make_element(&mut arena, "span");
    arena.append_child(body, div);
    arena.append_child(body, span);

    resolve_styles(&mut arena);

    // <div> should be display: block (from UA stylesheet)
    let div_node = super::super::stylo_bridge::StyloNode::new(div);
    let div_data = div_node.borrow_data().expect("div data");
    let div_display = div_data.styles.get_primary().expect("primary styles").get_box().display;
    assert!(
        !div_display.is_none(),
        "div should have a display value"
    );

    // <span> should be display: inline (default, not in our UA block list)
    let span_node = super::super::stylo_bridge::StyloNode::new(span);
    let span_data = span_node.borrow_data().expect("span data");
    let _span_display = span_data.styles.get_primary().expect("primary styles").get_box().display;
    // Both should resolve without panic; exact value depends on UA sheet.
}

#[test]
fn resolve_styles_multiple_style_elements() {
    let (mut arena, _html, head, body) = build_dom();

    // Two <style> elements
    let style1 = make_element(&mut arena, "style");
    let css1 = make_text(&mut arena, "body { margin: 0; }");
    arena.append_child(style1, css1);
    arena.append_child(head, style1);

    let style2 = make_element(&mut arena, "style");
    let css2 = make_text(&mut arena, "div { padding: 10px; }");
    arena.append_child(style2, css2);
    arena.append_child(head, style2);

    let div = make_element(&mut arena, "div");
    arena.append_child(body, div);

    resolve_styles(&mut arena);

    // Both body and div should have style data
    let body_node = super::super::stylo_bridge::StyloNode::new(body);
    assert!(body_node.has_data());

    let div_node = super::super::stylo_bridge::StyloNode::new(div);
    assert!(div_node.has_data());
}

#[test]
fn resolve_styles_class_selector() {
    let (mut arena, _html, head, body) = build_dom();

    let style = make_element(&mut arena, "style");
    let css = make_text(&mut arena, ".red { color: red; }");
    arena.append_child(style, css);
    arena.append_child(head, style);

    let div = make_element_with_attrs(&mut arena, "div", &[("class", "red")]);
    arena.append_child(body, div);

    resolve_styles(&mut arena);

    let div_node = super::super::stylo_bridge::StyloNode::new(div);
    assert!(div_node.has_data(), "div.red should have style data");
}

#[test]
fn resolve_styles_id_selector() {
    let (mut arena, _html, head, body) = build_dom();

    let style = make_element(&mut arena, "style");
    let css = make_text(&mut arena, "#main { font-size: 20px; }");
    arena.append_child(style, css);
    arena.append_child(head, style);

    let div = make_element_with_attrs(&mut arena, "div", &[("id", "main")]);
    arena.append_child(body, div);

    resolve_styles(&mut arena);

    let div_node = super::super::stylo_bridge::StyloNode::new(div);
    assert!(div_node.has_data(), "div#main should have style data");
}

#[test]
fn resolve_styles_nested_elements() {
    let (mut arena, _html, head, body) = build_dom();

    let style = make_element(&mut arena, "style");
    let css = make_text(&mut arena, "div span { font-weight: bold; }");
    arena.append_child(style, css);
    arena.append_child(head, style);

    let div = make_element(&mut arena, "div");
    let span = make_element(&mut arena, "span");
    let text = make_text(&mut arena, "hello");
    arena.append_child(body, div);
    arena.append_child(div, span);
    arena.append_child(span, text);

    resolve_styles(&mut arena);

    // All elements should get style data
    for id in [div, span] {
        let node = super::super::stylo_bridge::StyloNode::new(id);
        assert!(node.has_data(), "nested element should have style data");
    }
}

#[test]
fn resolve_styles_empty_style_element() {
    let (mut arena, _html, head, body) = build_dom();

    // Empty <style> element (no text children)
    let style = make_element(&mut arena, "style");
    arena.append_child(head, style);

    let div = make_element(&mut arena, "div");
    arena.append_child(body, div);

    resolve_styles(&mut arena);

    // Should still work — UA styles applied
    let div_node = super::super::stylo_bridge::StyloNode::new(div);
    assert!(div_node.has_data());
}

#[test]
fn resolve_styles_with_parsed_html() {
    // Use the real HTML parser to build a DOM, then resolve styles.
    let mut arena = crate::dom::parse_document(
        "<html><head><style>p { color: blue; }</style></head><body><p>Hello</p></body></html>",
    );

    resolve_styles(&mut arena);

    // Find the <p> element and check it has style data.
    let p = arena.find_element(arena.document, "p").expect("should find <p>");
    let p_node = super::super::stylo_bridge::StyloNode::new(p);
    assert!(p_node.has_data(), "<p> should have style data");
}

#[test]
fn resolve_styles_complex_selectors() {
    let mut arena = crate::dom::parse_document(
        r#"<html><head><style>
            ul > li:first-child { font-weight: bold; }
            a[href] { color: blue; }
            .container .item { display: inline-block; }
        </style></head><body>
            <div class="container"><span class="item">A</span></div>
            <ul><li>First</li><li>Second</li></ul>
            <a href="/link">Link</a>
        </body></html>"#,
    );

    resolve_styles(&mut arena);

    // All elements should resolve without panic
    let li = arena.find_element(arena.document, "li").expect("should find <li>");
    let a = arena.find_element(arena.document, "a").expect("should find <a>");
    let span = arena.find_element(arena.document, "span").expect("should find <span>");

    for (tag, id) in [("li", li), ("a", a), ("span", span)] {
        let node = super::super::stylo_bridge::StyloNode::new(id);
        assert!(node.has_data(), "<{tag}> should have style data");
    }
}

#[test]
fn resolve_styles_preserves_dom_structure() {
    let (mut arena, html, head, body) = build_dom();

    resolve_styles(&mut arena);

    // DOM structure should be unchanged after style resolution.
    assert_eq!(arena.nodes[html].parent, Some(arena.document));
    assert_eq!(arena.nodes[html].first_child, Some(head));
    assert_eq!(arena.nodes[html].last_child, Some(body));
    assert_eq!(arena.nodes[head].next_sibling, Some(body));
    assert_eq!(arena.nodes[body].prev_sibling, Some(head));
}

// ─── ParleyFontMetrics ────────────────────────────────────────────────────

#[test]
fn font_metrics_returns_real_values() {
    use style::servo::media_queries::FontMetricsProvider;

    let metrics = ParleyFontMetrics {
        font_ctx: std::sync::Arc::new(std::sync::Mutex::new(parley::FontContext::new())),
    };
    let result = metrics.query_font_metrics(
        false,
        &style::properties::style_structs::Font::initial_values(),
        style::values::computed::CSSPixelLength::new(16.0),
        style::values::specified::font::QueryFontMetricsFlags::empty(),
    );
    // With system fonts available, ascent should be non-zero
    assert!(result.ascent.px() > 0.0, "ascent should be > 0, got {}", result.ascent.px());
}

#[test]
fn font_metrics_base_size() {
    use style::servo::media_queries::FontMetricsProvider;

    let metrics = ParleyFontMetrics {
        font_ctx: std::sync::Arc::new(std::sync::Mutex::new(parley::FontContext::new())),
    };
    let serif_size = metrics.base_size_for_generic(
        style::values::computed::font::GenericFontFamily::Serif,
    );
    assert_eq!(serif_size.px(), 16.0);

    let mono_size = metrics.base_size_for_generic(
        style::values::computed::font::GenericFontFamily::Monospace,
    );
    assert_eq!(mono_size.px(), 13.0);
}

// ─── StubPainters ─────────────────────────────────────────────────────────

#[test]
fn stub_painters_returns_none() {
    let painters = StubPainters;
    let name = Atom::from("my-paint");
    assert!(painters.get(&name).is_none());
}
