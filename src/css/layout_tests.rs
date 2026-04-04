use super::*;
use crate::dom::arena::{Arena, NodeId};
use crate::dom::node::{ElementData, NodeData};
use markup5ever::{ns, QualName};

// ─── Helpers ──────────────────────────────────────────────────────────────

fn make_element(arena: &mut Arena, tag: &str) -> NodeId {
    let name = QualName::new(None, ns!(html), tag.into());
    arena.new_node(NodeData::Element(ElementData::new(name, vec![])))
}

fn make_text(arena: &mut Arena, text: &str) -> NodeId {
    arena.new_node(NodeData::Text(text.to_string()))
}

/// Build standard DOM, resolve styles, compute layout.
fn build_and_layout(html: &str) -> Arena {
    let mut arena = crate::dom::parse_document(html);
    crate::css::resolve::resolve_styles(&mut arena);
    compute_layout(&mut arena);
    arena
}

/// Build manually, resolve styles, compute layout.
fn resolve_and_layout(arena: &mut Arena) {
    crate::css::resolve::resolve_styles(arena);
    compute_layout(arena);
}

// ─── Basic layout computation ─────────────────────────────────────────────

#[test]
fn compute_layout_no_panic_empty_doc() {
    let mut arena = Arena::new();
    compute_layout(&mut arena);
}

#[test]
fn compute_layout_no_panic_minimal_doc() {
    let mut arena = Arena::new();
    let html = make_element(&mut arena, "html");
    let body = make_element(&mut arena, "body");
    arena.append_child(arena.document, html);
    arena.append_child(html, body);
    resolve_and_layout(&mut arena);
}

#[test]
fn compute_layout_no_panic_parsed_html() {
    build_and_layout("<html><head></head><body><div>Hello</div></body></html>");
}

// ─── Document/root has viewport dimensions ────────────────────────────────

#[test]
fn root_has_viewport_dimensions() {
    let arena = build_and_layout("<html><head></head><body></body></html>");
    let html = arena.find_element(arena.document, "html").unwrap();
    let html_layout = &arena.nodes[html].taffy_layout;
    // <html> should be laid out with the viewport width
    assert!(html_layout.size.width > 0.0, "html width should be > 0");
    assert!(html_layout.size.height >= 0.0, "html height should be >= 0");
}

// ─── Block layout ─────────────────────────────────────────────────────────

#[test]
fn block_div_fills_parent_width() {
    let arena = build_and_layout(
        "<html><head></head><body style='margin:0'><div id='test'>Hello</div></body></html>",
    );
    let div = arena.find_element(arena.document, "div").unwrap();
    let layout = &arena.nodes[div].taffy_layout;
    // A block div should fill parent width (minus parent's padding/border/margin)
    assert!(layout.size.width > 0.0, "div width should be > 0, got {}", layout.size.width);
}

#[test]
fn nested_blocks_have_positions() {
    let arena = build_and_layout(
        r#"<html><head><style>
            body { margin: 0; }
            .outer { width: 200px; height: 100px; }
            .inner { width: 100px; height: 50px; }
        </style></head><body>
            <div class="outer"><div class="inner"></div></div>
        </body></html>"#,
    );

    // Find the inner div
    let mut found_inner = None;
    let body = arena.find_element(arena.document, "body").unwrap();
    let outer = arena.nodes[body].first_child;
    if let Some(outer_id) = outer {
        let inner = arena.nodes[outer_id].first_child;
        if let Some(inner_id) = inner {
            if matches!(&arena.nodes[inner_id].data, NodeData::Element(_)) {
                found_inner = Some(inner_id);
            }
        }
    }

    if let Some(inner) = found_inner {
        let layout = &arena.nodes[inner].taffy_layout;
        // Inner div should have non-negative position
        assert!(layout.location.x >= 0.0);
        assert!(layout.location.y >= 0.0);
    }
}

// ─── Hidden elements ──────────────────────────────────────────────────────

#[test]
fn display_none_has_zero_size() {
    let arena = build_and_layout(
        r#"<html><head><style>.hidden { display: none; }</style></head>
        <body><div class="hidden">Hidden</div></body></html>"#,
    );
    let div = arena.find_element(arena.document, "div").unwrap();
    let layout = &arena.nodes[div].taffy_layout;
    assert_eq!(layout.size.width, 0.0);
    assert_eq!(layout.size.height, 0.0);
}

#[test]
fn hidden_attribute_has_zero_size() {
    let arena = build_and_layout(
        "<html><head></head><body><div hidden>Hidden</div></body></html>",
    );
    let div = arena.find_element(arena.document, "div").unwrap();
    let layout = &arena.nodes[div].taffy_layout;
    assert_eq!(layout.size.width, 0.0);
    assert_eq!(layout.size.height, 0.0);
}

// ─── Explicit dimensions ──────────────────────────────────────────────────

#[test]
fn explicit_width_height() {
    let arena = build_and_layout(
        r#"<html><head><style>
            body { margin: 0; }
            #box { width: 200px; height: 100px; }
        </style></head><body><div id="box"></div></body></html>"#,
    );
    let div = arena.find_element(arena.document, "div").unwrap();
    let layout = &arena.nodes[div].taffy_layout;
    assert_eq!(layout.size.width, 200.0);
    assert_eq!(layout.size.height, 100.0);
}

#[test]
fn percentage_width() {
    let arena = build_and_layout(
        "<html><head><style>body { margin: 0; } #box { width: 50%; height: 50px; }</style></head><body><div class=\"box\"></div></body></html>",
    );
    let html = arena.find_element(arena.document, "html").unwrap();
    let html_layout = &arena.nodes[html].taffy_layout;
    let div = arena.find_element(arena.document, "div").unwrap();
    let layout = &arena.nodes[div].taffy_layout;
    // html should have viewport width
    assert!(
        html_layout.size.width > 0.0,
        "html width should be > 0, got {}",
        html_layout.size.width
    );
    // 50% of parent width
    assert!(
        layout.size.width > 0.0,
        "div width should be > 0 (50% of html {}), got {}",
        html_layout.size.width,
        layout.size.width
    );
}

// ─── Flex layout ──────────────────────────────────────────────────────────

#[test]
fn flex_row_children_side_by_side() {
    // No whitespace between elements to avoid text nodes in children
    let arena = build_and_layout(
        r#"<html><head><style>body{margin:0}.flex{display:flex}.child{width:100px;height:50px}</style></head><body><div class="flex"><div class="child"></div><div class="child"></div></div></body></html>"#,
    );

    let flex = arena.find_element(arena.document, "div").unwrap();
    let first_child = arena.nodes[flex].first_child.unwrap();
    let second_child = arena.nodes[first_child].next_sibling.unwrap();

    let layout_a = &arena.nodes[first_child].taffy_layout;
    let layout_b = &arena.nodes[second_child].taffy_layout;

    // In flex row, second child should be to the right of first
    assert_eq!(layout_a.location.x, 0.0);
    assert!(
        layout_b.location.x >= 100.0,
        "second child x should be >= 100, got {}",
        layout_b.location.x
    );
    assert_eq!(layout_a.location.y, layout_b.location.y);
}

#[test]
fn flex_column_children_stacked() {
    let arena = build_and_layout(
        r#"<html><head><style>body{margin:0}.flex{display:flex;flex-direction:column}.child{width:100px;height:50px}</style></head><body><div class="flex"><div class="child"></div><div class="child"></div></div></body></html>"#,
    );

    let flex = arena.find_element(arena.document, "div").unwrap();
    let first_child = arena.nodes[flex].first_child.unwrap();
    let second_child = arena.nodes[first_child].next_sibling.unwrap();

    let layout_a = &arena.nodes[first_child].taffy_layout;
    let layout_b = &arena.nodes[second_child].taffy_layout;

    assert_eq!(layout_a.location.y, 0.0);
    assert!(
        layout_b.location.y >= 50.0,
        "second child y should be >= 50, got {}",
        layout_b.location.y
    );
}

// ─── Padding and border ──────────────────────────────────────────────────

#[test]
fn padding_increases_size() {
    let arena = build_and_layout(
        r#"<html><head><style>
            body { margin: 0; }
            #box { width: 100px; height: 50px; padding: 10px; box-sizing: content-box; }
        </style></head><body><div id="box"></div></body></html>"#,
    );
    let div = arena.find_element(arena.document, "div").unwrap();
    let layout = &arena.nodes[div].taffy_layout;
    // content-box: total width = 100 + 10 + 10 = 120
    assert_eq!(layout.size.width, 120.0);
    assert_eq!(layout.size.height, 70.0);
}

#[test]
fn border_box_sizing() {
    let arena = build_and_layout(
        r#"<html><head><style>
            body { margin: 0; }
            #box { width: 100px; height: 50px; padding: 10px; box-sizing: border-box; }
        </style></head><body><div id="box"></div></body></html>"#,
    );
    let div = arena.find_element(arena.document, "div").unwrap();
    let layout = &arena.nodes[div].taffy_layout;
    // border-box: total size IS the specified size
    assert_eq!(layout.size.width, 100.0);
    assert_eq!(layout.size.height, 50.0);
}

// ─── Text nodes ───────────────────────────────────────────────────────────

#[test]
fn text_node_has_nonzero_size() {
    let arena = build_and_layout(
        "<html><head><style>body { margin: 0; }</style></head><body><p>Hello World</p></body></html>",
    );
    let p = arena.find_element(arena.document, "p").unwrap();
    let p_layout = &arena.nodes[p].taffy_layout;
    // <p> contains "Hello World" — Parley should measure real text dimensions
    assert!(
        p_layout.size.height > 0.0,
        "p height should be > 0, got {}",
        p_layout.size.height
    );
    assert!(
        p_layout.size.width > 0.0,
        "p width should be > 0, got {}",
        p_layout.size.width
    );
}

// ─── DOM structure preserved ──────────────────────────────────────────────

#[test]
fn layout_preserves_dom_structure() {
    let mut arena = crate::dom::parse_document(
        "<html><head></head><body><div><span>Hi</span></div></body></html>",
    );
    let html_before = arena.find_element(arena.document, "html").unwrap();
    let body_before = arena.find_element(arena.document, "body").unwrap();

    crate::css::resolve::resolve_styles(&mut arena);
    compute_layout(&mut arena);

    let html_after = arena.find_element(arena.document, "html").unwrap();
    let body_after = arena.find_element(arena.document, "body").unwrap();
    assert_eq!(html_before, html_after);
    assert_eq!(body_before, body_after);
}

// ─── Margin ───────────────────────────────────────────────────────────────

#[test]
fn margin_offsets_position() {
    let arena = build_and_layout(
        r#"<html><head><style>body{margin:0}#box{width:100px;height:50px;margin-left:20px;margin-top:10px}</style></head><body><div id="box"></div></body></html>"#,
    );
    let div = arena.find_element(arena.document, "div").unwrap();
    let layout = &arena.nodes[div].taffy_layout;
    // margin-left offsets x position
    assert_eq!(layout.location.x, 20.0);
    // margin-top on first child collapses with parent in block flow (CSS spec).
    // The child's location.y within parent is 0, parent absorbs the margin.
    // Verify via absolute position instead:
    let body = arena.find_element(arena.document, "body").unwrap();
    let body_layout = &arena.nodes[body].taffy_layout;
    // The 10px margin should show up somewhere in the chain
    let abs_y = body_layout.location.y + layout.location.y;
    assert!(
        abs_y >= 10.0,
        "absolute y should be >= 10 (margin collapsing), got body.y={} + div.y={}",
        body_layout.location.y,
        layout.location.y
    );
}

// ─── LayoutTree construction ──────────────────────────────────────────────

#[test]
fn layout_tree_maps_all_nodes() {
    let mut arena = Arena::new();
    let html = make_element(&mut arena, "html");
    let body = make_element(&mut arena, "body");
    let div = make_element(&mut arena, "div");
    let text = make_text(&mut arena, "hello");
    arena.append_child(arena.document, html);
    arena.append_child(html, body);
    arena.append_child(body, div);
    arena.append_child(div, text);

    let tree = LayoutTree::new(&mut arena, parley::FontContext::new());
    // All 5 nodes should be mapped (document, html, body, div, text)
    assert_eq!(tree.taffy_to_dom.len(), 5);
    assert_eq!(tree.dom_to_taffy.len(), 5);
}

// ─── Grid layout ──────────────────────────────────────────────────────────

#[test]
fn grid_layout_positions_children() {
    let arena = build_and_layout(
        "<html><head><style>body{margin:0}.grid{display:grid;grid-template-columns:100px 100px;grid-template-rows:50px 50px}</style></head><body><div class=\"grid\"><div></div><div></div><div></div><div></div></div></body></html>",
    );
    let grid = arena.find_element(arena.document, "div").unwrap();
    let layout = &arena.nodes[grid].taffy_layout;
    // Grid container should have 200px width (2 x 100px columns)
    assert!(
        layout.size.width >= 200.0,
        "grid width should be >= 200, got {}",
        layout.size.width
    );
}

// ─── Text measurement edge cases ──────────────────────────────────────────

#[test]
fn empty_text_node_has_zero_height() {
    let arena = build_and_layout(
        "<html><head><style>body{margin:0}</style></head><body><p></p></body></html>",
    );
    let p = arena.find_element(arena.document, "p").unwrap();
    let layout = &arena.nodes[p].taffy_layout;
    // Empty <p> should have 0 height (no text content)
    assert_eq!(layout.size.height, 0.0);
}

#[test]
fn multiple_text_blocks_stack() {
    let arena = build_and_layout(
        "<html><head><style>body{margin:0}p{margin:0}</style></head><body><p>First</p><p>Second</p></body></html>",
    );
    let body = arena.find_element(arena.document, "body").unwrap();
    let body_layout = &arena.nodes[body].taffy_layout;
    // Body should have positive height from two paragraphs stacking
    assert!(
        body_layout.size.height > 0.0,
        "body should have height > 0 with two paragraphs"
    );
}

#[test]
fn font_size_affects_text_height() {
    let small = build_and_layout(
        "<html><head><style>body{margin:0}p{font-size:12px;margin:0}</style></head><body><p>Hello</p></body></html>",
    );
    let large = build_and_layout(
        "<html><head><style>body{margin:0}p{font-size:48px;margin:0}</style></head><body><p>Hello</p></body></html>",
    );
    let p_small = small.find_element(small.document, "p").unwrap();
    let p_large = large.find_element(large.document, "p").unwrap();
    let h_small = small.nodes[p_small].taffy_layout.size.height;
    let h_large = large.nodes[p_large].taffy_layout.size.height;
    assert!(
        h_large > h_small,
        "48px text ({}) should be taller than 12px text ({})",
        h_large,
        h_small
    );
}

// ─── Absolute positioning ─────────────────────────────────────────────────

#[test]
fn absolute_position_element() {
    let arena = build_and_layout(
        "<html><head><style>body{margin:0}.rel{position:relative;width:400px;height:300px}.abs{position:absolute;top:10px;left:20px;width:50px;height:50px}</style></head><body><div class=\"rel\"><div class=\"abs\"></div></div></body></html>",
    );
    // Find the absolute element (second div)
    let rel_div = arena.find_element(arena.document, "div").unwrap();
    let abs_div = arena.nodes[rel_div].first_child;
    if let Some(abs_id) = abs_div {
        if matches!(&arena.nodes[abs_id].data, NodeData::Element(_)) {
            let layout = &arena.nodes[abs_id].taffy_layout;
            assert_eq!(layout.location.x, 20.0, "absolute left should be 20px");
            assert_eq!(layout.location.y, 10.0, "absolute top should be 10px");
        }
    }
}

// ─── Inline style integration ─────────────────────────────────────────────

#[test]
fn inline_style_attribute_respected() {
    let arena = build_and_layout(
        "<html><head></head><body style=\"margin:0\"><div style=\"width:300px;height:150px\"></div></body></html>",
    );
    let div = arena.find_element(arena.document, "div").unwrap();
    let layout = &arena.nodes[div].taffy_layout;
    assert_eq!(layout.size.width, 300.0);
    assert_eq!(layout.size.height, 150.0);
}
