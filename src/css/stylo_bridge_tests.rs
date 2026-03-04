use super::*;
use crate::dom::arena::{Arena, NodeId};
use crate::dom::node::{ElementData, NodeData};
use markup5ever::{ns, Attribute, QualName};
use selectors::attr::{AttrSelectorOperation, AttrSelectorOperator, CaseSensitivity, NamespaceConstraint};
use selectors::Element as SelectorsElement;
use style::dom::{NodeInfo, TDocument, TElement, TNode};
use style_dom::ElementState;

// ─── Helpers ──────────────────────────────────────────────────────────────

fn make_element(arena: &mut Arena, tag: &str) -> NodeId {
    let name = QualName::new(None, ns!(html), tag.into());
    arena.new_node(NodeData::Element(ElementData::new(name, vec![])))
}

fn make_element_ns(arena: &mut Arena, tag: &str, ns_url: markup5ever::Namespace) -> NodeId {
    let name = QualName::new(None, ns_url, tag.into());
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

fn make_comment(arena: &mut Arena, text: &str) -> NodeId {
    arena.new_node(NodeData::Comment(text.to_string()))
}

/// Build a minimal DOM: Document → html → head + body
fn build_simple_dom() -> (Arena, NodeId, NodeId, NodeId) {
    let mut arena = Arena::new();
    let html = make_element(&mut arena, "html");
    let head = make_element(&mut arena, "head");
    let body = make_element(&mut arena, "body");
    arena.append_child(arena.document, html);
    arena.append_child(html, head);
    arena.append_child(html, body);
    (arena, html, head, body)
}

// ─── StyloNode basics ────────────────────────────────────────────────────

#[test]
fn stylo_node_new_and_access() {
    let arena = Arena::new();
    let node = StyloNode::new(&arena, arena.document);
    assert_eq!(node.id, arena.document);
}

#[test]
fn stylo_node_copy_clone() {
    let arena = Arena::new();
    let node = StyloNode::new(&arena, arena.document);
    let copy = node;
    let clone = node.clone();
    assert_eq!(node, copy);
    assert_eq!(node, clone);
}

#[test]
fn stylo_node_equality() {
    let mut arena = Arena::new();
    let div1 = make_element(&mut arena, "div");
    let div2 = make_element(&mut arena, "div");
    let n1 = StyloNode::new(&arena, div1);
    let n2 = StyloNode::new(&arena, div2);
    let n1_copy = StyloNode::new(&arena, div1);
    assert_eq!(n1, n1_copy);
    assert_ne!(n1, n2);
}

#[test]
fn stylo_node_hash() {
    use std::collections::HashSet;
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let span = make_element(&mut arena, "span");
    let mut set = HashSet::new();
    set.insert(StyloNode::new(&arena, div));
    set.insert(StyloNode::new(&arena, span));
    set.insert(StyloNode::new(&arena, div)); // duplicate
    assert_eq!(set.len(), 2);
}

#[test]
fn stylo_node_debug_format() {
    let arena = Arena::new();
    let node = StyloNode::new(&arena, arena.document);
    let debug = format!("{:?}", node);
    assert!(debug.starts_with("StyloNode("));
}

// ─── NodeInfo ─────────────────────────────────────────────────────────────

#[test]
fn node_info_is_element() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let text = make_text(&mut arena, "hello");
    assert!(StyloNode::new(&arena, div).is_element());
    assert!(!StyloNode::new(&arena, text).is_element());
    assert!(!StyloNode::new(&arena, arena.document).is_element());
}

#[test]
fn node_info_is_text_node() {
    let mut arena = Arena::new();
    let text = make_text(&mut arena, "hello");
    let div = make_element(&mut arena, "div");
    assert!(StyloNode::new(&arena, text).is_text_node());
    assert!(!StyloNode::new(&arena, div).is_text_node());
    assert!(!StyloNode::new(&arena, arena.document).is_text_node());
}

// ─── TDocument ────────────────────────────────────────────────────────────

#[test]
fn tdocument_as_node_returns_self() {
    let arena = Arena::new();
    let doc = StyloNode::new(&arena, arena.document);
    let node = TDocument::as_node(&doc);
    assert_eq!(node.id, doc.id);
}

#[test]
fn tdocument_is_html_document() {
    let arena = Arena::new();
    let doc = StyloNode::new(&arena, arena.document);
    assert!(doc.is_html_document());
}

#[test]
fn tdocument_quirks_mode_no_quirks() {
    let arena = Arena::new();
    let doc = StyloNode::new(&arena, arena.document);
    assert_eq!(doc.quirks_mode(), style::context::QuirksMode::NoQuirks);
}

// ─── TNode navigation ─────────────────────────────────────────────────────

#[test]
fn tnode_parent_node() {
    let (arena, html, _head, _body) = build_simple_dom();
    let html_node = StyloNode::new(&arena, html);
    let parent = html_node.parent_node().unwrap();
    assert_eq!(parent.id, arena.document);

    let doc_node = StyloNode::new(&arena, arena.document);
    assert!(doc_node.parent_node().is_none());
}

#[test]
fn tnode_first_last_child() {
    let (arena, html, head, body) = build_simple_dom();
    let html_node = StyloNode::new(&arena, html);
    assert_eq!(html_node.first_child().unwrap().id, head);
    assert_eq!(html_node.last_child().unwrap().id, body);

    let head_node = StyloNode::new(&arena, head);
    assert!(head_node.first_child().is_none());
    assert!(head_node.last_child().is_none());
}

#[test]
fn tnode_siblings() {
    let (arena, _html, head, body) = build_simple_dom();
    let head_node = StyloNode::new(&arena, head);
    let body_node = StyloNode::new(&arena, body);

    assert_eq!(head_node.next_sibling().unwrap().id, body);
    assert!(head_node.prev_sibling().is_none());
    assert!(body_node.next_sibling().is_none());
    assert_eq!(body_node.prev_sibling().unwrap().id, head);
}

#[test]
fn tnode_owner_doc() {
    let (arena, html, _head, _body) = build_simple_dom();
    let html_node = StyloNode::new(&arena, html);
    assert_eq!(html_node.owner_doc().id, arena.document);
}

#[test]
fn tnode_is_in_document() {
    let (arena, html, _head, _body) = build_simple_dom();
    let html_node = StyloNode::new(&arena, html);
    assert!(html_node.is_in_document());

    let mut arena2 = Arena::new();
    let detached = make_element(&mut arena2, "p");
    assert!(!StyloNode::new(&arena2, detached).is_in_document());
}

#[test]
fn tnode_traversal_parent() {
    let (arena, html, head, _body) = build_simple_dom();
    let head_node = StyloNode::new(&arena, head);
    let parent = head_node.traversal_parent().unwrap();
    assert_eq!(parent.id, html);

    // Root element's traversal parent is None (Document is not an element)
    let html_node = StyloNode::new(&arena, html);
    assert!(html_node.traversal_parent().is_none());
}

#[test]
fn tnode_opaque_and_debug_id() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let node = StyloNode::new(&arena, div);
    let opaque = TNode::opaque(&node);
    let debug_id = node.debug_id();
    // Should be consistent
    assert_eq!(opaque.0, debug_id);
}

#[test]
fn tnode_as_element_and_as_document() {
    let (arena, html, _head, _body) = build_simple_dom();
    let html_node = StyloNode::new(&arena, html);
    assert!(html_node.as_element().is_some());
    assert!(html_node.as_document().is_none());
    assert!(html_node.as_shadow_root().is_none());

    let doc_node = StyloNode::new(&arena, arena.document);
    assert!(doc_node.as_document().is_some());
    assert!(doc_node.as_element().is_none());
}

#[test]
fn tnode_text_node_is_not_element() {
    let mut arena = Arena::new();
    let text = make_text(&mut arena, "hello");
    let node = StyloNode::new(&arena, text);
    assert!(node.as_element().is_none());
    assert!(node.as_document().is_none());
}

// ─── selectors::Element — ID and class matching ───────────────────────────

#[test]
fn has_id_case_sensitive() {
    let mut arena = Arena::new();
    let div = make_element_with_attrs(&mut arena, "div", &[("id", "main")]);
    let node = StyloNode::new(&arena, div);

    let id_main = Atom::from("main");
    let id_ident = AtomIdent::cast(&id_main);
    assert!(node.has_id(id_ident, CaseSensitivity::CaseSensitive));

    let id_other = Atom::from("other");
    let id_other_ident = AtomIdent::cast(&id_other);
    assert!(!node.has_id(id_other_ident, CaseSensitivity::CaseSensitive));

    let id_main_upper = Atom::from("MAIN");
    let id_main_upper_ident = AtomIdent::cast(&id_main_upper);
    assert!(!node.has_id(id_main_upper_ident, CaseSensitivity::CaseSensitive));
    assert!(node.has_id(id_main_upper_ident, CaseSensitivity::AsciiCaseInsensitive));
}

#[test]
fn has_class_multiple() {
    let mut arena = Arena::new();
    let div = make_element_with_attrs(&mut arena, "div", &[("class", "foo bar baz")]);
    let node = StyloNode::new(&arena, div);

    for name in ["foo", "bar", "baz"] {
        let atom = Atom::from(name);
        let ident = AtomIdent::cast(&atom);
        assert!(node.has_class(ident, CaseSensitivity::CaseSensitive), "should have class {name}");
    }

    let atom = Atom::from("qux");
    let ident = AtomIdent::cast(&atom);
    assert!(!node.has_class(ident, CaseSensitivity::CaseSensitive));
}

#[test]
fn has_class_case_insensitive() {
    let mut arena = Arena::new();
    let div = make_element_with_attrs(&mut arena, "div", &[("class", "MyClass")]);
    let node = StyloNode::new(&arena, div);

    let atom = Atom::from("myclass");
    let ident = AtomIdent::cast(&atom);
    assert!(!node.has_class(ident, CaseSensitivity::CaseSensitive));
    assert!(node.has_class(ident, CaseSensitivity::AsciiCaseInsensitive));
}

#[test]
fn no_id_returns_false() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let node = StyloNode::new(&arena, div);

    let atom = Atom::from("anything");
    let ident = AtomIdent::cast(&atom);
    assert!(!node.has_id(ident, CaseSensitivity::CaseSensitive));
}

#[test]
fn no_class_returns_false() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let node = StyloNode::new(&arena, div);

    let atom = Atom::from("anything");
    let ident = AtomIdent::cast(&atom);
    assert!(!node.has_class(ident, CaseSensitivity::CaseSensitive));
}

// ─── selectors::Element — element navigation ──────────────────────────────

#[test]
fn sibling_element_navigation_skips_text() {
    let mut arena = Arena::new();
    let parent = make_element(&mut arena, "div");
    let a = make_element(&mut arena, "a");
    let text = make_text(&mut arena, "hello");
    let b = make_element(&mut arena, "b");
    arena.append_child(parent, a);
    arena.append_child(parent, text);
    arena.append_child(parent, b);

    let a_node = StyloNode::new(&arena, a);
    let b_node = StyloNode::new(&arena, b);

    // a's next sibling element skips text, finds b
    assert_eq!(a_node.next_sibling_element().unwrap().id, b);
    // b's prev sibling element skips text, finds a
    assert_eq!(b_node.prev_sibling_element().unwrap().id, a);
}

#[test]
fn first_element_child_skips_text() {
    let mut arena = Arena::new();
    let parent = make_element(&mut arena, "div");
    let text = make_text(&mut arena, "hello");
    let span = make_element(&mut arena, "span");
    arena.append_child(parent, text);
    arena.append_child(parent, span);

    let parent_node = StyloNode::new(&arena, parent);
    assert_eq!(parent_node.first_element_child().unwrap().id, span);
}

#[test]
fn first_element_child_none_when_only_text() {
    let mut arena = Arena::new();
    let parent = make_element(&mut arena, "div");
    let text = make_text(&mut arena, "hello");
    arena.append_child(parent, text);

    let parent_node = StyloNode::new(&arena, parent);
    assert!(parent_node.first_element_child().is_none());
}

#[test]
fn parent_element_returns_none_at_root() {
    let (arena, html, _head, _body) = build_simple_dom();
    let html_node = StyloNode::new(&arena, html);
    // html's parent is Document, which is not an element
    assert!(html_node.parent_element().is_none());
}

#[test]
fn parent_element_returns_parent_for_nested() {
    let (arena, html, head, _body) = build_simple_dom();
    let head_node = StyloNode::new(&arena, head);
    assert_eq!(head_node.parent_element().unwrap().id, html);
}

// ─── selectors::Element — is_root, is_empty, is_link ──────────────────────

#[test]
fn is_root_for_html_element() {
    let (arena, html, head, _body) = build_simple_dom();
    let html_node = StyloNode::new(&arena, html);
    assert!(html_node.is_root());

    let head_node = StyloNode::new(&arena, head);
    assert!(!head_node.is_root());
}

#[test]
fn is_empty_with_no_children() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    assert!(StyloNode::new(&arena, div).is_empty());
}

#[test]
fn is_empty_with_empty_text_child() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let text = make_text(&mut arena, "");
    arena.append_child(div, text);
    assert!(StyloNode::new(&arena, div).is_empty());
}

#[test]
fn is_empty_false_with_text_content() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let text = make_text(&mut arena, "hello");
    arena.append_child(div, text);
    assert!(!StyloNode::new(&arena, div).is_empty());
}

#[test]
fn is_empty_false_with_element_child() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let span = make_element(&mut arena, "span");
    arena.append_child(div, span);
    assert!(!StyloNode::new(&arena, div).is_empty());
}

#[test]
fn is_link_anchor_with_href() {
    let mut arena = Arena::new();
    let a = make_element_with_attrs(&mut arena, "a", &[("href", "https://example.com")]);
    assert!(StyloNode::new(&arena, a).is_link());
}

#[test]
fn is_link_anchor_without_href() {
    let mut arena = Arena::new();
    let a = make_element(&mut arena, "a");
    assert!(!StyloNode::new(&arena, a).is_link());
}

#[test]
fn is_link_area_with_href() {
    let mut arena = Arena::new();
    let area = make_element_with_attrs(&mut arena, "area", &[("href", "/map")]);
    assert!(StyloNode::new(&arena, area).is_link());
}

#[test]
fn is_link_includes_link_element() {
    let mut arena = Arena::new();
    let link = make_element_with_attrs(&mut arena, "link", &[("href", "/style.css")]);
    assert!(StyloNode::new(&arena, link).is_link());
}

#[test]
fn is_link_div_is_false() {
    let mut arena = Arena::new();
    let div = make_element_with_attrs(&mut arena, "div", &[("href", "/fake")]);
    assert!(!StyloNode::new(&arena, div).is_link());
}

// ─── selectors::Element — attr_matches ────────────────────────────────────

#[test]
fn attr_matches_exists() {
    let mut arena = Arena::new();
    let div = make_element_with_attrs(&mut arena, "div", &[("data-x", "1")]);
    let node = StyloNode::new(&arena, div);

    let local = LocalName::from("data-x");
    assert!(node.attr_matches(
        &NamespaceConstraint::Any,
        &local,
        &AttrSelectorOperation::Exists,
    ));

    let missing = LocalName::from("data-y");
    assert!(!node.attr_matches(
        &NamespaceConstraint::Any,
        &missing,
        &AttrSelectorOperation::Exists,
    ));
}

#[test]
fn attr_matches_equal() {
    let mut arena = Arena::new();
    let div = make_element_with_attrs(&mut arena, "div", &[("lang", "en")]);
    let node = StyloNode::new(&arena, div);

    let local = LocalName::from("lang");
    let val = style::values::AtomString::from("en");
    assert!(node.attr_matches(
        &NamespaceConstraint::Any,
        &local,
        &AttrSelectorOperation::WithValue {
            operator: AttrSelectorOperator::Equal,
            case_sensitivity: CaseSensitivity::CaseSensitive,
            value: &val,
        },
    ));

    let wrong_val = style::values::AtomString::from("fr");
    assert!(!node.attr_matches(
        &NamespaceConstraint::Any,
        &local,
        &AttrSelectorOperation::WithValue {
            operator: AttrSelectorOperator::Equal,
            case_sensitivity: CaseSensitivity::CaseSensitive,
            value: &wrong_val,
        },
    ));
}

#[test]
fn attr_matches_includes() {
    let mut arena = Arena::new();
    let div = make_element_with_attrs(&mut arena, "div", &[("class", "foo bar baz")]);
    let node = StyloNode::new(&arena, div);

    let local = LocalName::from("class");
    let val = style::values::AtomString::from("bar");
    assert!(node.attr_matches(
        &NamespaceConstraint::Any,
        &local,
        &AttrSelectorOperation::WithValue {
            operator: AttrSelectorOperator::Includes,
            case_sensitivity: CaseSensitivity::CaseSensitive,
            value: &val,
        },
    ));
}

#[test]
fn attr_matches_prefix() {
    let mut arena = Arena::new();
    let div = make_element_with_attrs(&mut arena, "div", &[("data-x", "hello-world")]);
    let node = StyloNode::new(&arena, div);

    let local = LocalName::from("data-x");
    let val = style::values::AtomString::from("hello");
    assert!(node.attr_matches(
        &NamespaceConstraint::Any,
        &local,
        &AttrSelectorOperation::WithValue {
            operator: AttrSelectorOperator::Prefix,
            case_sensitivity: CaseSensitivity::CaseSensitive,
            value: &val,
        },
    ));
}

#[test]
fn attr_matches_suffix() {
    let mut arena = Arena::new();
    let div = make_element_with_attrs(&mut arena, "div", &[("data-x", "hello-world")]);
    let node = StyloNode::new(&arena, div);

    let local = LocalName::from("data-x");
    let val = style::values::AtomString::from("world");
    assert!(node.attr_matches(
        &NamespaceConstraint::Any,
        &local,
        &AttrSelectorOperation::WithValue {
            operator: AttrSelectorOperator::Suffix,
            case_sensitivity: CaseSensitivity::CaseSensitive,
            value: &val,
        },
    ));
}

#[test]
fn attr_matches_substring() {
    let mut arena = Arena::new();
    let div = make_element_with_attrs(&mut arena, "div", &[("data-x", "hello-world")]);
    let node = StyloNode::new(&arena, div);

    let local = LocalName::from("data-x");
    let val = style::values::AtomString::from("lo-wo");
    assert!(node.attr_matches(
        &NamespaceConstraint::Any,
        &local,
        &AttrSelectorOperation::WithValue {
            operator: AttrSelectorOperator::Substring,
            case_sensitivity: CaseSensitivity::CaseSensitive,
            value: &val,
        },
    ));
}

#[test]
fn attr_matches_dash_match() {
    let mut arena = Arena::new();
    let div = make_element_with_attrs(&mut arena, "div", &[("lang", "en-US")]);
    let node = StyloNode::new(&arena, div);

    let local = LocalName::from("lang");
    let val = style::values::AtomString::from("en");
    assert!(node.attr_matches(
        &NamespaceConstraint::Any,
        &local,
        &AttrSelectorOperation::WithValue {
            operator: AttrSelectorOperator::DashMatch,
            case_sensitivity: CaseSensitivity::CaseSensitive,
            value: &val,
        },
    ));
}

// ─── selectors::Element — pseudo-classes ──────────────────────────────────

#[test]
fn match_pseudo_class_link_and_any_link() {
    let mut arena = Arena::new();
    let a = make_element_with_attrs(&mut arena, "a", &[("href", "/page")]);
    let node = StyloNode::new(&arena, a);

    use selectors::matching::{MatchingContext, MatchingMode, NeedsSelectorInvalidation};
    let mut ctx = MatchingContext::<SelectorImpl>::new(
        MatchingMode::Normal,
        None,
        NeedsSelectorInvalidation::No,
        style::context::QuirksMode::NoQuirks,
    );

    assert!(node.match_non_ts_pseudo_class(&NonTSPseudoClass::Link, &mut ctx));
    assert!(node.match_non_ts_pseudo_class(&NonTSPseudoClass::AnyLink, &mut ctx));
}

#[test]
fn match_pseudo_class_defined() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let node = StyloNode::new(&arena, div);

    use selectors::matching::{MatchingContext, MatchingMode, NeedsSelectorInvalidation};
    let mut ctx = MatchingContext::<SelectorImpl>::new(
        MatchingMode::Normal,
        None,
        NeedsSelectorInvalidation::No,
        style::context::QuirksMode::NoQuirks,
    );

    assert!(node.match_non_ts_pseudo_class(&NonTSPseudoClass::Defined, &mut ctx));
}

#[test]
fn match_pseudo_class_hover_default_false() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let node = StyloNode::new(&arena, div);

    use selectors::matching::{MatchingContext, MatchingMode, NeedsSelectorInvalidation};
    let mut ctx = MatchingContext::<SelectorImpl>::new(
        MatchingMode::Normal,
        None,
        NeedsSelectorInvalidation::No,
        style::context::QuirksMode::NoQuirks,
    );

    // Default state has no HOVER
    assert!(!node.match_non_ts_pseudo_class(&NonTSPseudoClass::Hover, &mut ctx));
    assert!(!node.match_non_ts_pseudo_class(&NonTSPseudoClass::Focus, &mut ctx));
    assert!(!node.match_non_ts_pseudo_class(&NonTSPseudoClass::Active, &mut ctx));
}

#[test]
fn match_pseudo_class_with_state() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    arena.nodes[div].element_state = ElementState::HOVER | ElementState::FOCUS;
    let node = StyloNode::new(&arena, div);

    use selectors::matching::{MatchingContext, MatchingMode, NeedsSelectorInvalidation};
    let mut ctx = MatchingContext::<SelectorImpl>::new(
        MatchingMode::Normal,
        None,
        NeedsSelectorInvalidation::No,
        style::context::QuirksMode::NoQuirks,
    );

    assert!(node.match_non_ts_pseudo_class(&NonTSPseudoClass::Hover, &mut ctx));
    assert!(node.match_non_ts_pseudo_class(&NonTSPseudoClass::Focus, &mut ctx));
    assert!(!node.match_non_ts_pseudo_class(&NonTSPseudoClass::Active, &mut ctx));
}

// ─── selectors::Element — is_html_element_in_html_document ───────────────

#[test]
fn is_html_element_in_html_document() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    assert!(StyloNode::new(&arena, div).is_html_element_in_html_document());

    let svg = make_element_ns(&mut arena, "rect", ns!(svg));
    assert!(!StyloNode::new(&arena, svg).is_html_element_in_html_document());
}

// ─── selectors::Element — has_local_name, has_namespace ──────────────────

#[test]
fn has_local_name() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let node = StyloNode::new(&arena, div);

    let div_name = web_atoms::LocalName::from("div");
    assert!(node.has_local_name(&div_name));

    let span_name = web_atoms::LocalName::from("span");
    assert!(!node.has_local_name(&span_name));
}

#[test]
fn has_namespace_html() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let node = StyloNode::new(&arena, div);

    let html_ns = web_atoms::Namespace::from("http://www.w3.org/1999/xhtml");
    assert!(node.has_namespace(&html_ns));

    let svg_ns = web_atoms::Namespace::from("http://www.w3.org/2000/svg");
    assert!(!node.has_namespace(&svg_ns));
}

// ─── selectors::Element — is_same_type ────────────────────────────────────

#[test]
fn is_same_type() {
    let mut arena = Arena::new();
    let div1 = make_element(&mut arena, "div");
    let div2 = make_element(&mut arena, "div");
    let span = make_element(&mut arena, "span");

    let n1 = StyloNode::new(&arena, div1);
    let n2 = StyloNode::new(&arena, div2);
    let n3 = StyloNode::new(&arena, span);

    assert!(n1.is_same_type(&n2));
    assert!(!n1.is_same_type(&n3));
}

// ─── selectors::Element — opaque ──────────────────────────────────────────

#[test]
fn selectors_opaque_different_for_different_nodes() {
    let mut arena = Arena::new();
    let a = make_element(&mut arena, "a");
    let b = make_element(&mut arena, "b");
    let a_opaque = SelectorsElement::opaque(&StyloNode::new(&arena, a));
    let b_opaque = SelectorsElement::opaque(&StyloNode::new(&arena, b));
    assert_ne!(a_opaque, b_opaque);
}

// ─── TElement ─────────────────────────────────────────────────────────────

#[test]
fn telement_as_node_returns_self() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let node = StyloNode::new(&arena, div);
    assert_eq!(TElement::as_node(&node).id, div);
}

#[test]
fn telement_traversal_children() {
    let (arena, html, head, body) = build_simple_dom();
    let html_node = StyloNode::new(&arena, html);
    let children: Vec<_> = html_node.traversal_children().map(|n| n.id).collect();
    assert_eq!(children, vec![head, body]);
}

#[test]
fn telement_traversal_children_empty() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let node = StyloNode::new(&arena, div);
    let children: Vec<_> = node.traversal_children().collect();
    assert!(children.is_empty());
}

#[test]
fn telement_is_html_svg_mathml() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let svg = make_element_ns(&mut arena, "svg", ns!(svg));
    let math = make_element_ns(&mut arena, "math", ns!(mathml));

    assert!(StyloNode::new(&arena, div).is_html_element());
    assert!(!StyloNode::new(&arena, div).is_svg_element());
    assert!(!StyloNode::new(&arena, div).is_mathml_element());

    assert!(StyloNode::new(&arena, svg).is_svg_element());
    assert!(!StyloNode::new(&arena, svg).is_html_element());

    assert!(StyloNode::new(&arena, math).is_mathml_element());
    assert!(!StyloNode::new(&arena, math).is_html_element());
}

#[test]
fn telement_state() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    assert_eq!(StyloNode::new(&arena, div).state(), ElementState::empty());

    arena.nodes[div].element_state = ElementState::HOVER;
    assert_eq!(StyloNode::new(&arena, div).state(), ElementState::HOVER);
}

#[test]
fn telement_ensure_and_clear_data() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let node = StyloNode::new(&arena, div);

    assert!(!node.has_data());
    assert!(node.borrow_data().is_none());

    // ensure_data creates ElementData
    unsafe { node.ensure_data() };
    assert!(node.has_data());
    assert!(node.borrow_data().is_some());

    // clear_data removes it
    unsafe { node.clear_data() };
    assert!(!node.has_data());
}

#[test]
fn telement_dirty_descendants() {
    let mut arena = Arena::new();
    let parent = make_element(&mut arena, "div");
    let child = make_element(&mut arena, "span");
    arena.append_child(parent, child);

    let child_node = StyloNode::new(&arena, child);
    assert!(!child_node.has_dirty_descendants());

    unsafe { child_node.set_dirty_descendants() };
    assert!(child_node.has_dirty_descendants());
    // Parent should also be marked dirty
    assert!(StyloNode::new(&arena, parent).has_dirty_descendants());

    unsafe { child_node.unset_dirty_descendants() };
    assert!(!child_node.has_dirty_descendants());
}

#[test]
fn telement_each_class() {
    let mut arena = Arena::new();
    let div = make_element_with_attrs(&mut arena, "div", &[("class", "alpha beta gamma")]);
    let node = StyloNode::new(&arena, div);

    let mut classes = Vec::new();
    node.each_class(|cls| {
        classes.push(cls.to_string());
    });
    assert_eq!(classes, vec!["alpha", "beta", "gamma"]);
}

#[test]
fn telement_each_class_no_class_attr() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let node = StyloNode::new(&arena, div);

    let mut classes = Vec::new();
    node.each_class(|cls| {
        classes.push(cls.to_string());
    });
    assert!(classes.is_empty());
}

#[test]
fn telement_each_attr_name() {
    let mut arena = Arena::new();
    let div = make_element_with_attrs(&mut arena, "div", &[("id", "x"), ("class", "y"), ("data-foo", "z")]);
    let node = StyloNode::new(&arena, div);

    let mut names = Vec::new();
    node.each_attr_name(|name| {
        names.push(name.to_string());
    });
    assert_eq!(names.len(), 3);
    assert!(names.contains(&"id".to_string()));
    assert!(names.contains(&"class".to_string()));
    assert!(names.contains(&"data-foo".to_string()));
}

#[test]
fn telement_is_html_document_body_element() {
    let (arena, _html, _head, body) = build_simple_dom();
    let body_node = StyloNode::new(&arena, body);
    assert!(body_node.is_html_document_body_element());

    let (arena2, _html2, head2, _body2) = build_simple_dom();
    let head_node = StyloNode::new(&arena2, head2);
    assert!(!head_node.is_html_document_body_element());
}

#[test]
fn telement_style_attribute_returns_none() {
    let mut arena = Arena::new();
    let div = make_element_with_attrs(&mut arena, "div", &[("style", "color: red")]);
    let node = StyloNode::new(&arena, div);
    // TODO: Not yet implemented — always returns None
    assert!(node.style_attribute().is_none());
}

#[test]
fn telement_no_animations() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let node = StyloNode::new(&arena, div);
    assert!(!node.may_have_animations());
}

#[test]
fn telement_no_shadow_root() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let node = StyloNode::new(&arena, div);
    assert!(node.shadow_root().is_none());
    assert!(node.containing_shadow().is_none());
}

#[test]
fn telement_no_part() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let node = StyloNode::new(&arena, div);
    assert!(!node.has_part_attr());
    assert!(!node.exports_any_part());
}

#[test]
fn telement_local_name_and_namespace() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let node = StyloNode::new(&arena, div);

    let local = node.local_name();
    assert_eq!(&**local, "div");

    let ns = node.namespace();
    assert_eq!(&**ns, "http://www.w3.org/1999/xhtml");
}

#[test]
fn telement_selector_flags() {
    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let node = StyloNode::new(&arena, div);

    assert!(!node.has_selector_flags(ElementSelectorFlags::HAS_SLOW_SELECTOR));
    assert_eq!(node.relative_selector_search_direction(), ElementSelectorFlags::empty());
}

// ─── AttributeProvider ───────────────────────────────────────────────────

#[test]
fn attribute_provider_get_attr() {
    let mut arena = Arena::new();
    let div = make_element_with_attrs(&mut arena, "div", &[("id", "test"), ("class", "foo")]);
    let node = StyloNode::new(&arena, div);

    let id_name = LocalName::from("id");
    assert_eq!(node.get_attr(&id_name).as_deref(), Some("test"));

    let class_name = LocalName::from("class");
    assert_eq!(node.get_attr(&class_name).as_deref(), Some("foo"));

    let missing = LocalName::from("data-x");
    assert!(node.get_attr(&missing).is_none());
}

#[test]
fn attribute_provider_non_element_returns_none() {
    let mut arena = Arena::new();
    let text = make_text(&mut arena, "hello");
    let node = StyloNode::new(&arena, text);
    let name = LocalName::from("anything");
    assert!(node.get_attr(&name).is_none());
}

// ─── parse_size_attr ──────────────────────────────────────────────────────

#[test]
fn parse_size_attr_pixels() {
    let result = parse_size_attr("100");
    assert!(result.is_some());
}

#[test]
fn parse_size_attr_px_suffix() {
    let result = parse_size_attr("200px");
    assert!(result.is_some());
}

#[test]
fn parse_size_attr_percentage() {
    let result = parse_size_attr("50%");
    assert!(result.is_some());
}

#[test]
fn parse_size_attr_invalid() {
    assert!(parse_size_attr("abc").is_none());
    assert!(parse_size_attr("").is_none());
}

#[test]
fn parse_size_attr_negative() {
    assert!(parse_size_attr("-10").is_none());
}

#[test]
fn parse_size_attr_zero() {
    let result = parse_size_attr("0");
    assert!(result.is_some());
}

// ─── Presentational hints ─────────────────────────────────────────────────

#[test]
fn presentational_hints_hidden_attribute() {
    use selectors::matching::VisitedHandlingMode;
    use style::applicable_declarations::ApplicableDeclarationBlock;

    let mut arena = Arena::new();
    let div = make_element_with_attrs(&mut arena, "div", &[("hidden", "")]);
    let node = StyloNode::new(&arena, div);

    let mut hints: Vec<ApplicableDeclarationBlock> = Vec::new();
    node.synthesize_presentational_hints_for_legacy_attributes(
        VisitedHandlingMode::AllLinksUnvisited,
        &mut hints,
    );
    // Should produce a display:none hint
    assert_eq!(hints.len(), 1);
}

#[test]
fn presentational_hints_width_height_on_img() {
    use selectors::matching::VisitedHandlingMode;
    use style::applicable_declarations::ApplicableDeclarationBlock;

    let mut arena = Arena::new();
    let img = make_element_with_attrs(&mut arena, "img", &[("width", "200"), ("height", "100")]);
    let node = StyloNode::new(&arena, img);

    let mut hints: Vec<ApplicableDeclarationBlock> = Vec::new();
    node.synthesize_presentational_hints_for_legacy_attributes(
        VisitedHandlingMode::AllLinksUnvisited,
        &mut hints,
    );
    // Should produce width + height hints
    assert_eq!(hints.len(), 2);
}

#[test]
fn presentational_hints_no_hints_for_plain_div() {
    use selectors::matching::VisitedHandlingMode;
    use style::applicable_declarations::ApplicableDeclarationBlock;

    let mut arena = Arena::new();
    let div = make_element(&mut arena, "div");
    let node = StyloNode::new(&arena, div);

    let mut hints: Vec<ApplicableDeclarationBlock> = Vec::new();
    node.synthesize_presentational_hints_for_legacy_attributes(
        VisitedHandlingMode::AllLinksUnvisited,
        &mut hints,
    );
    assert!(hints.is_empty());
}

#[test]
fn presentational_hints_width_on_non_supported_element() {
    use selectors::matching::VisitedHandlingMode;
    use style::applicable_declarations::ApplicableDeclarationBlock;

    let mut arena = Arena::new();
    // <p> doesn't support width/height presentational hints
    let p = make_element_with_attrs(&mut arena, "p", &[("width", "100")]);
    let node = StyloNode::new(&arena, p);

    let mut hints: Vec<ApplicableDeclarationBlock> = Vec::new();
    node.synthesize_presentational_hints_for_legacy_attributes(
        VisitedHandlingMode::AllLinksUnvisited,
        &mut hints,
    );
    assert!(hints.is_empty());
}

// ─── apply_selector_flags ─────────────────────────────────────────────────

#[test]
fn apply_selector_flags_on_self_and_parent() {
    let mut arena = Arena::new();
    let parent = make_element(&mut arena, "div");
    let child = make_element(&mut arena, "span");
    arena.append_child(parent, child);

    let child_node = StyloNode::new(&arena, child);

    // Apply flags that include both self and parent flags
    child_node.apply_selector_flags(
        ElementSelectorFlags::HAS_SLOW_SELECTOR | ElementSelectorFlags::HAS_SLOW_SELECTOR_LATER_SIBLINGS,
    );

    // Check that self flag was applied
    assert!(arena.nodes[child].selector_flags.get().contains(ElementSelectorFlags::HAS_SLOW_SELECTOR));
}
