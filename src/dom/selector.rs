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
#[path = "selector_tests.rs"]
mod tests;

