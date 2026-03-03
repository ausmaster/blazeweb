//! CSS selector matching via the `selectors` crate.
//!
//! Provides querySelector, querySelectorAll, matches, and closest
//! by wrapping our Arena-based DOM in the traits the selectors crate expects.

use std::fmt;

use cssparser::ToCss;
use precomputed_hash::PrecomputedHash;
use selectors::attr::{AttrSelectorOperation, CaseSensitivity, NamespaceConstraint};
use selectors::context::{MatchingContext, MatchingForInvalidation, MatchingMode, QuirksMode, SelectorCaches};
use selectors::matching::{matches_selector_list, ElementSelectorFlags};
use selectors::parser::{self, NonTSPseudoClass, ParseRelative, PseudoElement, SelectorImpl, SelectorList, SelectorParseErrorKind};
use selectors::{matching::NeedsSelectorFlags, OpaqueElement};

use super::arena::{Arena, NodeId};
use super::node::NodeData;

// ─── Newtype wrappers ────────────────────────────────────────────────────────

/// Wrapper for local names / identifiers / attr values used by the selectors crate.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct CssAtom(pub String);

impl From<&str> for CssAtom {
    fn from(s: &str) -> Self {
        CssAtom(s.to_owned())
    }
}

impl ToCss for CssAtom {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        cssparser::serialize_identifier(&self.0, dest)
    }
}

impl PrecomputedHash for CssAtom {
    fn precomputed_hash(&self) -> u32 {
        let mut h: u32 = 5381;
        for b in self.0.bytes() {
            h = h.wrapping_mul(33).wrapping_add(b as u32);
        }
        h
    }
}

impl AsRef<str> for CssAtom {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Attribute value wrapper — needs partial matching support.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct CssAttrValue(pub String);

impl From<&str> for CssAttrValue {
    fn from(s: &str) -> Self {
        CssAttrValue(s.to_owned())
    }
}

impl ToCss for CssAttrValue {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        cssparser::serialize_string(&self.0, dest)
    }
}

impl AsRef<str> for CssAttrValue {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

// ─── SelectorImpl ────────────────────────────────────────────────────────────

/// Our SelectorImpl — connects the selectors crate to our DOM types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlazeSelector;

impl SelectorImpl for BlazeSelector {
    type ExtraMatchingData<'a> = ();
    type AttrValue = CssAttrValue;
    type Identifier = CssAtom;
    type LocalName = CssAtom;
    type NamespaceUrl = CssAtom;
    type NamespacePrefix = CssAtom;
    type BorrowedLocalName = CssAtom;
    type BorrowedNamespaceUrl = CssAtom;
    type NonTSPseudoClass = BlazePseudoClass;
    type PseudoElement = BlazePseudoElement;
}

// ─── Pseudo-class (empty — SSR has no user-action states) ────────────────────

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BlazePseudoClass {}

impl ToCss for BlazePseudoClass {
    fn to_css<W: fmt::Write>(&self, _dest: &mut W) -> fmt::Result {
        match *self {}
    }
}

impl NonTSPseudoClass for BlazePseudoClass {
    type Impl = BlazeSelector;

    fn is_active_or_hover(&self) -> bool {
        match *self {}
    }

    fn is_user_action_state(&self) -> bool {
        match *self {}
    }
}

// ─── Pseudo-element (empty — SSR doesn't render pseudo-elements) ─────────────

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BlazePseudoElement {}

impl ToCss for BlazePseudoElement {
    fn to_css<W: fmt::Write>(&self, _dest: &mut W) -> fmt::Result {
        match *self {}
    }
}

impl PseudoElement for BlazePseudoElement {
    type Impl = BlazeSelector;
}

// ─── Parser ──────────────────────────────────────────────────────────────────

/// Minimal selector parser — supports :is(), :where(), :has(), :not().
pub struct BlazeSelectorParser;

impl<'i> parser::Parser<'i> for BlazeSelectorParser {
    type Impl = BlazeSelector;
    type Error = SelectorParseErrorKind<'i>;

    fn parse_is_and_where(&self) -> bool {
        true
    }

    fn parse_has(&self) -> bool {
        true
    }

    fn parse_nth_child_of(&self) -> bool {
        true
    }
}

// ─── ArenaElement — wraps a NodeId + Arena ref for selectors::Element ────────

/// Lightweight element handle for the selectors crate.
///
/// Points into an Arena (by raw pointer) and a specific NodeId.
/// Only valid while the Arena is alive (guaranteed by call-site).
#[derive(Clone)]
pub struct ArenaElement {
    arena: *const Arena,
    pub node_id: NodeId,
}

impl fmt::Debug for ArenaElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ArenaElement({:?})", self.node_id)
    }
}

impl ArenaElement {
    /// Create a new ArenaElement. Caller must ensure `arena` outlives usage.
    pub fn new(arena: &Arena, node_id: NodeId) -> Self {
        Self {
            arena: arena as *const Arena,
            node_id,
        }
    }

    fn arena(&self) -> &Arena {
        unsafe { &*self.arena }
    }

    fn elem_data(&self) -> Option<&crate::dom::node::ElementData> {
        self.arena().element_data(self.node_id)
    }
}

impl PartialEq for ArenaElement {
    fn eq(&self, other: &Self) -> bool {
        self.node_id == other.node_id
    }
}

impl Eq for ArenaElement {}

impl selectors::Element for ArenaElement {
    type Impl = BlazeSelector;

    fn opaque(&self) -> OpaqueElement {
        OpaqueElement::new(self)
    }

    fn parent_element(&self) -> Option<Self> {
        let arena = self.arena();
        let parent_id = arena.nodes[self.node_id].parent?;
        // Parent must be an element (not Document)
        if matches!(&arena.nodes[parent_id].data, NodeData::Element(_)) {
            Some(ArenaElement::new(arena, parent_id))
        } else {
            None
        }
    }

    fn parent_node_is_shadow_root(&self) -> bool {
        false
    }

    fn containing_shadow_host(&self) -> Option<Self> {
        None
    }

    fn is_pseudo_element(&self) -> bool {
        false
    }

    fn prev_sibling_element(&self) -> Option<Self> {
        let arena = self.arena();
        let mut current = arena.nodes[self.node_id].prev_sibling;
        while let Some(id) = current {
            if matches!(&arena.nodes[id].data, NodeData::Element(_)) {
                return Some(ArenaElement::new(arena, id));
            }
            current = arena.nodes[id].prev_sibling;
        }
        None
    }

    fn next_sibling_element(&self) -> Option<Self> {
        let arena = self.arena();
        let mut current = arena.nodes[self.node_id].next_sibling;
        while let Some(id) = current {
            if matches!(&arena.nodes[id].data, NodeData::Element(_)) {
                return Some(ArenaElement::new(arena, id));
            }
            current = arena.nodes[id].next_sibling;
        }
        None
    }

    fn first_element_child(&self) -> Option<Self> {
        let arena = self.arena();
        let mut current = arena.nodes[self.node_id].first_child;
        while let Some(id) = current {
            if matches!(&arena.nodes[id].data, NodeData::Element(_)) {
                return Some(ArenaElement::new(arena, id));
            }
            current = arena.nodes[id].next_sibling;
        }
        None
    }

    fn is_html_element_in_html_document(&self) -> bool {
        if let Some(data) = self.elem_data() {
            data.name.ns == markup5ever::ns!(html)
        } else {
            false
        }
    }

    fn has_local_name(&self, local_name: &CssAtom) -> bool {
        if let Some(data) = self.elem_data() {
            *data.name.local == *local_name.0
        } else {
            false
        }
    }

    fn has_namespace(&self, ns: &CssAtom) -> bool {
        if let Some(data) = self.elem_data() {
            if ns.0.is_empty() {
                // Empty namespace matches HTML namespace
                data.name.ns == markup5ever::ns!(html) || data.name.ns == markup5ever::ns!()
            } else {
                *data.name.ns == *ns.0
            }
        } else {
            false
        }
    }

    fn is_same_type(&self, other: &Self) -> bool {
        let a = self.elem_data();
        let b = other.elem_data();
        match (a, b) {
            (Some(a), Some(b)) => a.name.local == b.name.local && a.name.ns == b.name.ns,
            _ => false,
        }
    }

    fn attr_matches(
        &self,
        ns: &NamespaceConstraint<&CssAtom>,
        local_name: &CssAtom,
        operation: &AttrSelectorOperation<&CssAttrValue>,
    ) -> bool {
        let Some(data) = self.elem_data() else {
            return false;
        };
        for attr in &data.attrs {
            // Check namespace constraint
            match ns {
                NamespaceConstraint::Any => {}
                NamespaceConstraint::Specific(ns_url) => {
                    if ns_url.0.is_empty() {
                        // No namespace
                        if attr.name.ns != markup5ever::ns!() {
                            continue;
                        }
                    } else if *attr.name.ns != *ns_url.0 {
                        continue;
                    }
                }
            }
            // Check local name
            if *attr.name.local != *local_name.0 {
                continue;
            }
            // Check operation
            let value = &*attr.value;
            let matches = match operation {
                AttrSelectorOperation::Exists => true,
                AttrSelectorOperation::WithValue {
                    operator,
                    case_sensitivity,
                    value: expected,
                } => {
                    let val = value;
                    let exp = expected.0.as_str();
                    let eq = |a: &str, b: &str| -> bool {
                        match *case_sensitivity {
                            CaseSensitivity::CaseSensitive => a == b,
                            CaseSensitivity::AsciiCaseInsensitive => a.eq_ignore_ascii_case(b),
                        }
                    };
                    use selectors::attr::AttrSelectorOperator;
                    match operator {
                        AttrSelectorOperator::Equal => eq(val, exp),
                        AttrSelectorOperator::Includes => {
                            val.split_whitespace().any(|w| eq(w, exp))
                        }
                        AttrSelectorOperator::DashMatch => {
                            eq(val, exp)
                                || (val.len() > exp.len()
                                    && eq(&val[..exp.len()], exp)
                                    && val.as_bytes()[exp.len()] == b'-')
                        }
                        AttrSelectorOperator::Prefix => {
                            !exp.is_empty() && {
                                val.len() >= exp.len() && eq(&val[..exp.len()], exp)
                            }
                        }
                        AttrSelectorOperator::Suffix => {
                            !exp.is_empty() && {
                                val.len() >= exp.len() && eq(&val[val.len()-exp.len()..], exp)
                            }
                        }
                        AttrSelectorOperator::Substring => {
                            !exp.is_empty() && {
                                if *case_sensitivity == CaseSensitivity::CaseSensitive {
                                    val.contains(exp)
                                } else {
                                    val.to_ascii_lowercase().contains(&exp.to_ascii_lowercase())
                                }
                            }
                        }
                    }
                }
            };
            if matches {
                return true;
            }
        }
        false
    }

    fn match_non_ts_pseudo_class(
        &self,
        _pc: &BlazePseudoClass,
        _context: &mut MatchingContext<BlazeSelector>,
    ) -> bool {
        // Unreachable — BlazePseudoClass is empty enum
        match *_pc {}
    }

    fn match_pseudo_element(
        &self,
        _pe: &BlazePseudoElement,
        _context: &mut MatchingContext<BlazeSelector>,
    ) -> bool {
        // Empty enum — unreachable
        false
    }

    fn apply_selector_flags(&self, _flags: ElementSelectorFlags) {
        // No-op for SSR
    }

    fn is_link(&self) -> bool {
        if let Some(data) = self.elem_data() {
            let tag = &*data.name.local;
            (tag == "a" || tag == "area" || tag == "link") && data.get_attribute("href").is_some()
        } else {
            false
        }
    }

    fn is_html_slot_element(&self) -> bool {
        if let Some(data) = self.elem_data() {
            *data.name.local == *"slot" && data.name.ns == markup5ever::ns!(html)
        } else {
            false
        }
    }

    fn has_id(&self, id: &CssAtom, case_sensitivity: CaseSensitivity) -> bool {
        if let Some(data) = self.elem_data() {
            if let Some(elem_id) = data.get_attribute("id") {
                case_sensitivity.eq(elem_id.as_bytes(), id.0.as_bytes())
            } else {
                false
            }
        } else {
            false
        }
    }

    fn has_class(&self, name: &CssAtom, case_sensitivity: CaseSensitivity) -> bool {
        if let Some(data) = self.elem_data() {
            if let Some(class_attr) = data.get_attribute("class") {
                class_attr.split_whitespace().any(|cls| {
                    case_sensitivity.eq(cls.as_bytes(), name.0.as_bytes())
                })
            } else {
                false
            }
        } else {
            false
        }
    }

    fn has_custom_state(&self, _name: &CssAtom) -> bool {
        false
    }

    fn imported_part(&self, _name: &CssAtom) -> Option<CssAtom> {
        None
    }

    fn is_part(&self, _name: &CssAtom) -> bool {
        false
    }

    fn is_empty(&self) -> bool {
        let arena = self.arena();
        // Element is :empty if it has no element or text children
        let mut child = arena.nodes[self.node_id].first_child;
        while let Some(id) = child {
            match &arena.nodes[id].data {
                NodeData::Element(_) => return false,
                NodeData::Text(t) => {
                    if !t.is_empty() {
                        return false;
                    }
                }
                _ => {}
            }
            child = arena.nodes[id].next_sibling;
        }
        true
    }

    fn is_root(&self) -> bool {
        let arena = self.arena();
        match arena.nodes[self.node_id].parent {
            Some(parent_id) => matches!(&arena.nodes[parent_id].data, NodeData::Document),
            None => false,
        }
    }

    fn add_element_unique_hashes(&self, _filter: &mut selectors::bloom::BloomFilter) -> bool {
        // Return false to indicate we didn't add hashes (bloom filter optimization skipped)
        false
    }
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Parse a CSS selector string. Returns error message on parse failure.
fn parse_selector(selector: &str) -> Result<SelectorList<BlazeSelector>, String> {
    let mut input = cssparser::ParserInput::new(selector);
    let mut parser = cssparser::Parser::new(&mut input);
    SelectorList::parse(&BlazeSelectorParser, &mut parser, ParseRelative::No)
        .map_err(|e| format!("Invalid selector: {:?}", e))
}

/// Find the first descendant element matching the selector.
pub fn query_selector(arena: &Arena, root: NodeId, selector: &str) -> Result<Option<NodeId>, String> {
    let selector_list = parse_selector(selector)?;
    let mut caches = SelectorCaches::default();
    let mut context = MatchingContext::new(
        MatchingMode::Normal,
        None,
        &mut caches,
        QuirksMode::NoQuirks,
        NeedsSelectorFlags::No,
        MatchingForInvalidation::No,
    );

    // Depth-first walk of descendants
    let result = query_selector_recursive(arena, root, &selector_list, &mut context, true);
    Ok(result.into_iter().next())
}

/// Find all descendant elements matching the selector (document order).
pub fn query_selector_all(arena: &Arena, root: NodeId, selector: &str) -> Result<Vec<NodeId>, String> {
    let selector_list = parse_selector(selector)?;
    let mut caches = SelectorCaches::default();
    let mut context = MatchingContext::new(
        MatchingMode::Normal,
        None,
        &mut caches,
        QuirksMode::NoQuirks,
        NeedsSelectorFlags::No,
        MatchingForInvalidation::No,
    );

    Ok(query_selector_recursive(arena, root, &selector_list, &mut context, false))
}

/// Check if an element matches a selector.
pub fn matches_element(arena: &Arena, node_id: NodeId, selector: &str) -> Result<bool, String> {
    if !matches!(&arena.nodes[node_id].data, NodeData::Element(_)) {
        return Ok(false);
    }
    let selector_list = parse_selector(selector)?;
    let mut caches = SelectorCaches::default();
    let mut context = MatchingContext::new(
        MatchingMode::Normal,
        None,
        &mut caches,
        QuirksMode::NoQuirks,
        NeedsSelectorFlags::No,
        MatchingForInvalidation::No,
    );

    let elem = ArenaElement::new(arena, node_id);
    Ok(matches_selector_list(&selector_list, &elem, &mut context))
}

/// Find the closest ancestor (or self) matching the selector.
pub fn closest(arena: &Arena, node_id: NodeId, selector: &str) -> Result<Option<NodeId>, String> {
    let selector_list = parse_selector(selector)?;
    let mut caches = SelectorCaches::default();
    let mut context = MatchingContext::new(
        MatchingMode::Normal,
        None,
        &mut caches,
        QuirksMode::NoQuirks,
        NeedsSelectorFlags::No,
        MatchingForInvalidation::No,
    );

    let mut current = Some(node_id);
    while let Some(id) = current {
        if matches!(&arena.nodes[id].data, NodeData::Element(_)) {
            let elem = ArenaElement::new(arena, id);
            if matches_selector_list(&selector_list, &elem, &mut context) {
                return Ok(Some(id));
            }
        }
        current = arena.nodes[id].parent;
    }
    Ok(None)
}

fn query_selector_recursive(
    arena: &Arena,
    node: NodeId,
    selector_list: &SelectorList<BlazeSelector>,
    context: &mut MatchingContext<BlazeSelector>,
    first_only: bool,
) -> Vec<NodeId> {
    let mut results = Vec::new();
    query_descendants(arena, node, selector_list, context, first_only, &mut results);
    results
}

fn query_descendants(
    arena: &Arena,
    node: NodeId,
    selector_list: &SelectorList<BlazeSelector>,
    context: &mut MatchingContext<BlazeSelector>,
    first_only: bool,
    results: &mut Vec<NodeId>,
) {
    for child in arena.children(node) {
        if first_only && !results.is_empty() {
            return;
        }
        if matches!(&arena.nodes[child].data, NodeData::Element(_)) {
            // Fresh caches per match to avoid nth-child cache invalidation across siblings
            let mut caches = SelectorCaches::default();
            let mut ctx = MatchingContext::new(
                MatchingMode::Normal,
                None,
                &mut caches,
                QuirksMode::NoQuirks,
                NeedsSelectorFlags::No,
                MatchingForInvalidation::No,
            );
            let elem = ArenaElement::new(arena, child);
            if matches_selector_list(selector_list, &elem, &mut ctx) {
                results.push(child);
                if first_only {
                    return;
                }
            }
        }
        // Recurse into children regardless of whether parent matched
        query_descendants(arena, child, selector_list, context, first_only, results);
    }
}

#[cfg(test)]
mod tests {
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
}
