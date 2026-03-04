//! Stylo trait bridge — implements TDocument, TNode, TElement for our Arena-based DOM.
//!
//! We use a lightweight `StyloNode` wrapper (arena ref + NodeId) that is Copy/Clone,
//! as required by Stylo's trait bounds. The same type implements all Stylo traits,
//! with runtime dispatch based on node type (following Blitz's pattern).

use std::hash::{Hash, Hasher};
use std::sync::atomic::Ordering;

use atomic_refcell::{AtomicRef, AtomicRefMut};
use selectors::attr::{AttrSelectorOperation, CaseSensitivity, NamespaceConstraint};
use selectors::matching::{ElementSelectorFlags, MatchingContext, VisitedHandlingMode};
use selectors::sink::Push;
use selectors::{Element as SelectorsElement, OpaqueElement};
use slotmap::Key;
use style::applicable_declarations::ApplicableDeclarationBlock;
use style::context::SharedStyleContext;
use style::dom::{
    AttributeProvider, LayoutIterator, NodeInfo, OpaqueNode, TDocument, TElement, TNode,
    TShadowRoot,
};
use style::properties::PropertyDeclarationBlock;
use style::selector_parser::{NonTSPseudoClass, PseudoElement, SelectorImpl};
use style::servo_arc::{Arc, ArcBorrow};
use style::shared_lock::{Locked, SharedRwLock};
use style::values::AtomIdent;
use style::{Atom, LocalName, Namespace};
use style_dom::ElementState;

use crate::dom::arena::{Arena, NodeId};
use crate::dom::node::NodeData;

// ─── StyloNode wrapper ──────────────────────────────────────────────────────

/// Lightweight handle into our Arena for Stylo's trait system.
///
/// Copy + Clone + Sized as required by TNode/TElement. Navigation is done via
/// the Arena reference — no back-pointers needed in Node.
#[derive(Copy, Clone)]
pub struct StyloNode<'a> {
    pub arena: &'a Arena,
    pub id: NodeId,
}

impl<'a> StyloNode<'a> {
    pub fn new(arena: &'a Arena, id: NodeId) -> Self {
        Self { arena, id }
    }

    #[inline]
    fn node(&self) -> &'a crate::dom::arena::Node {
        &self.arena.nodes[self.id]
    }

    fn elem_data(&self) -> Option<&'a crate::dom::node::ElementData> {
        self.arena.element_data(self.id)
    }
}

impl std::fmt::Debug for StyloNode<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StyloNode({:?})", self.id)
    }
}

impl PartialEq for StyloNode<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for StyloNode<'_> {}

impl Hash for StyloNode<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

// ─── NodeInfo ────────────────────────────────────────────────────────────────

impl NodeInfo for StyloNode<'_> {
    fn is_element(&self) -> bool {
        matches!(&self.node().data, NodeData::Element(_))
    }

    fn is_text_node(&self) -> bool {
        matches!(&self.node().data, NodeData::Text(_))
    }
}

// ─── TDocument ───────────────────────────────────────────────────────────────

impl<'a> TDocument for StyloNode<'a> {
    type ConcreteNode = StyloNode<'a>;

    fn as_node(&self) -> Self::ConcreteNode {
        *self
    }

    fn is_html_document(&self) -> bool {
        true
    }

    fn quirks_mode(&self) -> style::context::QuirksMode {
        style::context::QuirksMode::NoQuirks
    }

    fn shared_lock(&self) -> &SharedRwLock {
        &self.arena.guard
    }
}

// ─── TShadowRoot ─────────────────────────────────────────────────────────────

impl<'a> TShadowRoot for StyloNode<'a> {
    type ConcreteNode = StyloNode<'a>;

    fn as_node(&self) -> Self::ConcreteNode {
        *self
    }

    fn host(&self) -> <Self::ConcreteNode as TNode>::ConcreteElement {
        unreachable!("Shadow DOM not supported")
    }

    fn style_data<'b>(&self) -> Option<&'b style::stylist::CascadeData>
    where
        Self: 'b,
    {
        None
    }
}

// ─── TNode ───────────────────────────────────────────────────────────────────

impl<'a> TNode for StyloNode<'a> {
    type ConcreteElement = StyloNode<'a>;
    type ConcreteDocument = StyloNode<'a>;
    type ConcreteShadowRoot = StyloNode<'a>;

    fn parent_node(&self) -> Option<Self> {
        self.node().parent.map(|id| StyloNode::new(self.arena, id))
    }

    fn first_child(&self) -> Option<Self> {
        self.node()
            .first_child
            .map(|id| StyloNode::new(self.arena, id))
    }

    fn last_child(&self) -> Option<Self> {
        self.node()
            .last_child
            .map(|id| StyloNode::new(self.arena, id))
    }

    fn prev_sibling(&self) -> Option<Self> {
        self.node()
            .prev_sibling
            .map(|id| StyloNode::new(self.arena, id))
    }

    fn next_sibling(&self) -> Option<Self> {
        self.node()
            .next_sibling
            .map(|id| StyloNode::new(self.arena, id))
    }

    fn owner_doc(&self) -> Self::ConcreteDocument {
        StyloNode::new(self.arena, self.arena.document)
    }

    fn is_in_document(&self) -> bool {
        self.node().flags.is_connected()
    }

    fn traversal_parent(&self) -> Option<Self::ConcreteElement> {
        self.parent_node().and_then(|n| n.as_element())
    }

    fn opaque(&self) -> OpaqueNode {
        OpaqueNode(self.id.data().as_ffi() as usize)
    }

    fn debug_id(self) -> usize {
        self.id.data().as_ffi() as usize
    }

    fn as_element(&self) -> Option<Self::ConcreteElement> {
        if matches!(&self.node().data, NodeData::Element(_)) {
            Some(*self)
        } else {
            None
        }
    }

    fn as_document(&self) -> Option<Self::ConcreteDocument> {
        if matches!(&self.node().data, NodeData::Document) {
            Some(*self)
        } else {
            None
        }
    }

    fn as_shadow_root(&self) -> Option<Self::ConcreteShadowRoot> {
        None
    }
}

// ─── Children iterator for TElement::traversal_children ──────────────────────

pub struct StyloChildIter<'a> {
    arena: &'a Arena,
    current: Option<NodeId>,
}

impl<'a> Iterator for StyloChildIter<'a> {
    type Item = StyloNode<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.current?;
        self.current = self.arena.nodes[id].next_sibling;
        Some(StyloNode::new(self.arena, id))
    }
}

// ─── AttributeProvider ───────────────────────────────────────────────────────

impl AttributeProvider for StyloNode<'_> {
    fn get_attr(&self, attr: &LocalName) -> Option<String> {
        let elem = self.elem_data()?;
        // Compare by string value: our markup5ever::LocalName vs style::LocalName (web_atoms)
        let attr_name: &str = attr.as_ref();
        elem.get_attribute(attr_name).map(|s| s.to_string())
    }
}

// ─── selectors::Element (Stylo's SelectorImpl) ──────────────────────────────

impl SelectorsElement for StyloNode<'_> {
    type Impl = SelectorImpl;

    fn opaque(&self) -> OpaqueElement {
        // Encode NodeId as a fake non-null pointer (like Blitz).
        // +1 to avoid null for the first slot.
        let ffi = self.id.data().as_ffi() as usize + 1;
        let non_null = std::ptr::NonNull::new(ffi as *mut ()).unwrap();
        OpaqueElement::from_non_null_ptr(non_null)
    }

    fn parent_element(&self) -> Option<Self> {
        TElement::traversal_parent(self)
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
        let mut current = self.node().prev_sibling;
        while let Some(id) = current {
            if matches!(&self.arena.nodes[id].data, NodeData::Element(_)) {
                return Some(StyloNode::new(self.arena, id));
            }
            current = self.arena.nodes[id].prev_sibling;
        }
        None
    }

    fn next_sibling_element(&self) -> Option<Self> {
        let mut current = self.node().next_sibling;
        while let Some(id) = current {
            if matches!(&self.arena.nodes[id].data, NodeData::Element(_)) {
                return Some(StyloNode::new(self.arena, id));
            }
            current = self.arena.nodes[id].next_sibling;
        }
        None
    }

    fn first_element_child(&self) -> Option<Self> {
        let mut current = self.node().first_child;
        while let Some(id) = current {
            if matches!(&self.arena.nodes[id].data, NodeData::Element(_)) {
                return Some(StyloNode::new(self.arena, id));
            }
            current = self.arena.nodes[id].next_sibling;
        }
        None
    }

    fn is_html_element_in_html_document(&self) -> bool {
        self.elem_data()
            .map(|d| d.name.ns == markup5ever::ns!(html))
            .unwrap_or(false)
    }

    fn has_local_name(&self, local_name: &web_atoms::LocalName) -> bool {
        self.elem_data()
            .map(|d| &*d.name.local == &**local_name)
            .unwrap_or(false)
    }

    fn has_namespace(&self, ns: &web_atoms::Namespace) -> bool {
        self.elem_data()
            .map(|d| &*d.name.ns == &**ns)
            .unwrap_or(false)
    }

    fn is_same_type(&self, other: &Self) -> bool {
        match (self.elem_data(), other.elem_data()) {
            (Some(a), Some(b)) => {
                a.name.local == b.name.local && a.name.ns == b.name.ns
            }
            _ => false,
        }
    }

    fn attr_matches(
        &self,
        ns: &NamespaceConstraint<&Namespace>,
        local_name: &LocalName,
        operation: &AttrSelectorOperation<&style::values::AtomString>,
    ) -> bool {
        let Some(data) = self.elem_data() else {
            return false;
        };
        let local_str: &str = local_name.as_ref();
        for attr in &data.attrs {
            // Check namespace constraint
            match ns {
                NamespaceConstraint::Any => {}
                NamespaceConstraint::Specific(ns_url) => {
                    let ns_str: &str = ns_url.as_ref();
                    if ns_str.is_empty() {
                        if attr.name.ns != markup5ever::ns!() {
                            continue;
                        }
                    } else if &*attr.name.ns != ns_str {
                        continue;
                    }
                }
            }
            // Check local name
            if &*attr.name.local != local_str {
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
                    let exp: &str = expected.as_ref();
                    use selectors::attr::AttrSelectorOperator;
                    let eq = |a: &str, b: &str| -> bool {
                        match *case_sensitivity {
                            CaseSensitivity::CaseSensitive => a == b,
                            CaseSensitivity::AsciiCaseInsensitive => {
                                a.eq_ignore_ascii_case(b)
                            }
                        }
                    };
                    match operator {
                        AttrSelectorOperator::Equal => eq(value, exp),
                        AttrSelectorOperator::Includes => {
                            value.split_whitespace().any(|w| eq(w, exp))
                        }
                        AttrSelectorOperator::DashMatch => {
                            eq(value, exp)
                                || (value.len() > exp.len()
                                    && eq(&value[..exp.len()], exp)
                                    && value.as_bytes()[exp.len()] == b'-')
                        }
                        AttrSelectorOperator::Prefix => {
                            !exp.is_empty()
                                && value.len() >= exp.len()
                                && eq(&value[..exp.len()], exp)
                        }
                        AttrSelectorOperator::Suffix => {
                            !exp.is_empty()
                                && value.len() >= exp.len()
                                && eq(&value[value.len() - exp.len()..], exp)
                        }
                        AttrSelectorOperator::Substring => {
                            !exp.is_empty() && {
                                if *case_sensitivity == CaseSensitivity::CaseSensitive {
                                    value.contains(exp)
                                } else {
                                    value
                                        .to_ascii_lowercase()
                                        .contains(&exp.to_ascii_lowercase())
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
        pseudo_class: &NonTSPseudoClass,
        _context: &mut MatchingContext<SelectorImpl>,
    ) -> bool {
        let state = self.node().element_state;
        match *pseudo_class {
            NonTSPseudoClass::Active => state.contains(ElementState::ACTIVE),
            NonTSPseudoClass::Focus => state.contains(ElementState::FOCUS),
            NonTSPseudoClass::Hover => state.contains(ElementState::HOVER),
            NonTSPseudoClass::Enabled => state.contains(ElementState::ENABLED),
            NonTSPseudoClass::Disabled => state.contains(ElementState::DISABLED),
            NonTSPseudoClass::Checked => state.contains(ElementState::CHECKED),
            NonTSPseudoClass::Visited => state.contains(ElementState::VISITED),
            NonTSPseudoClass::Link => {
                // :link matches unvisited links
                self.elem_data()
                    .map(|d| {
                        let tag = &*d.name.local;
                        (tag == "a" || tag == "area") && d.get_attribute("href").is_some()
                    })
                    .unwrap_or(false)
            }
            NonTSPseudoClass::AnyLink => self
                .elem_data()
                .map(|d| {
                    let tag = &*d.name.local;
                    (tag == "a" || tag == "area") && d.get_attribute("href").is_some()
                })
                .unwrap_or(false),
            NonTSPseudoClass::Defined => {
                // All standard HTML elements are :defined
                self.elem_data()
                    .map(|d| d.name.ns == markup5ever::ns!(html))
                    .unwrap_or(false)
            }
            // For SSR, most interactive pseudo-classes return false
            _ => false,
        }
    }

    fn match_pseudo_element(
        &self,
        _pe: &PseudoElement,
        _context: &mut MatchingContext<SelectorImpl>,
    ) -> bool {
        false
    }

    fn apply_selector_flags(&self, flags: ElementSelectorFlags) {
        let self_flags = flags.for_self();
        if !self_flags.is_empty() {
            let node = self.node();
            node.selector_flags
                .set(node.selector_flags.get() | self_flags);
        }
        let parent_flags = flags.for_parent();
        if !parent_flags.is_empty() {
            if let Some(parent_id) = self.node().parent {
                let parent = &self.arena.nodes[parent_id];
                parent
                    .selector_flags
                    .set(parent.selector_flags.get() | parent_flags);
            }
        }
    }

    fn is_link(&self) -> bool {
        self.elem_data()
            .map(|d| {
                let tag = &*d.name.local;
                (tag == "a" || tag == "area" || tag == "link")
                    && d.get_attribute("href").is_some()
            })
            .unwrap_or(false)
    }

    fn is_html_slot_element(&self) -> bool {
        false
    }

    fn has_id(
        &self,
        id: &<SelectorImpl as selectors::SelectorImpl>::Identifier,
        case_sensitivity: CaseSensitivity,
    ) -> bool {
        self.elem_data()
            .and_then(|d| d.get_attribute("id"))
            .map(|elem_id| {
                let id_str: &str = id.as_ref();
                match case_sensitivity {
                    CaseSensitivity::CaseSensitive => elem_id == id_str,
                    CaseSensitivity::AsciiCaseInsensitive => {
                        elem_id.eq_ignore_ascii_case(id_str)
                    }
                }
            })
            .unwrap_or(false)
    }

    fn has_class(
        &self,
        name: &<SelectorImpl as selectors::SelectorImpl>::Identifier,
        case_sensitivity: CaseSensitivity,
    ) -> bool {
        self.elem_data()
            .and_then(|d| d.get_attribute("class"))
            .map(|class_attr| {
                let name_str: &str = name.as_ref();
                class_attr.split_whitespace().any(|cls| match case_sensitivity {
                    CaseSensitivity::CaseSensitive => cls == name_str,
                    CaseSensitivity::AsciiCaseInsensitive => {
                        cls.eq_ignore_ascii_case(name_str)
                    }
                })
            })
            .unwrap_or(false)
    }

    fn has_custom_state(
        &self,
        _name: &<SelectorImpl as selectors::SelectorImpl>::Identifier,
    ) -> bool {
        false
    }

    fn imported_part(
        &self,
        _name: &<SelectorImpl as selectors::SelectorImpl>::Identifier,
    ) -> Option<<SelectorImpl as selectors::SelectorImpl>::Identifier> {
        None
    }

    fn is_part(
        &self,
        _name: &<SelectorImpl as selectors::SelectorImpl>::Identifier,
    ) -> bool {
        false
    }

    fn is_empty(&self) -> bool {
        let mut child = self.node().first_child;
        while let Some(id) = child {
            match &self.arena.nodes[id].data {
                NodeData::Element(_) => return false,
                NodeData::Text(t) if !t.is_empty() => return false,
                _ => {}
            }
            child = self.arena.nodes[id].next_sibling;
        }
        true
    }

    fn is_root(&self) -> bool {
        // Root element's parent is the Document node.
        self.node()
            .parent
            .map(|pid| matches!(&self.arena.nodes[pid].data, NodeData::Document))
            .unwrap_or(false)
    }

    fn add_element_unique_hashes(&self, _filter: &mut selectors::bloom::BloomFilter) -> bool {
        // Skip bloom filter optimization for now.
        false
    }
}

// ─── TElement ────────────────────────────────────────────────────────────────

impl<'a> TElement for StyloNode<'a> {
    type ConcreteNode = StyloNode<'a>;
    type TraversalChildrenIterator = StyloChildIter<'a>;

    fn as_node(&self) -> Self::ConcreteNode {
        *self
    }

    fn traversal_children(&self) -> LayoutIterator<Self::TraversalChildrenIterator> {
        LayoutIterator(StyloChildIter {
            arena: self.arena,
            current: self.node().first_child,
        })
    }

    fn is_html_element(&self) -> bool {
        self.elem_data()
            .map(|d| d.name.ns == markup5ever::ns!(html))
            .unwrap_or(false)
    }

    fn is_mathml_element(&self) -> bool {
        self.elem_data()
            .map(|d| d.name.ns == markup5ever::ns!(mathml))
            .unwrap_or(false)
    }

    fn is_svg_element(&self) -> bool {
        self.elem_data()
            .map(|d| d.name.ns == markup5ever::ns!(svg))
            .unwrap_or(false)
    }

    fn style_attribute(&self) -> Option<ArcBorrow<'_, Locked<PropertyDeclarationBlock>>> {
        // TODO: Parse inline style="" attributes into PropertyDeclarationBlock.
        // For now, return None (no inline style support).
        None
    }

    fn state(&self) -> ElementState {
        self.node().element_state
    }

    fn has_part_attr(&self) -> bool {
        false
    }

    fn exports_any_part(&self) -> bool {
        false
    }

    fn id(&self) -> Option<&Atom> {
        // TODO: Cache the interned atom on the element.
        // For now, we cannot return a reference to a temporary Atom.
        // This means ID-based selectors won't work through Stylo's fast path,
        // but they'll still work through the selectors::Element::has_id path.
        None
    }

    fn each_class<F>(&self, mut callback: F)
    where
        F: FnMut(&AtomIdent),
    {
        if let Some(class_attr) = self.elem_data().and_then(|d| d.get_attribute("class")) {
            for cls in class_attr.split_whitespace() {
                let atom = Atom::from(cls);
                callback(AtomIdent::cast(&atom));
            }
        }
    }

    fn each_custom_state<F>(&self, _callback: F)
    where
        F: FnMut(&AtomIdent),
    {
    }

    fn each_attr_name<F>(&self, mut callback: F)
    where
        F: FnMut(&LocalName),
    {
        if let Some(data) = self.elem_data() {
            for attr in &data.attrs {
                let web_local = web_atoms::LocalName::from(&*attr.name.local);
                let local_name = style::values::GenericAtomIdent(web_local);
                callback(&local_name);
            }
        }
    }

    fn has_dirty_descendants(&self) -> bool {
        self.node().dirty_descendants.load(Ordering::Relaxed)
    }

    fn has_snapshot(&self) -> bool {
        false // No snapshot support for SSR
    }

    fn handled_snapshot(&self) -> bool {
        true // Always "handled" since we never create snapshots
    }

    unsafe fn set_handled_snapshot(&self) {}

    unsafe fn set_dirty_descendants(&self) {
        self.node()
            .dirty_descendants
            .store(true, Ordering::Relaxed);
        // Walk up ancestors, setting dirty_descendants on each
        let mut current = self.node().parent;
        while let Some(id) = current {
            let node = &self.arena.nodes[id];
            if node.dirty_descendants.load(Ordering::Relaxed) {
                break; // Already dirty, ancestors must be too
            }
            node.dirty_descendants.store(true, Ordering::Relaxed);
            current = node.parent;
        }
    }

    unsafe fn unset_dirty_descendants(&self) {
        self.node()
            .dirty_descendants
            .store(false, Ordering::Relaxed);
    }

    fn store_children_to_process(&self, _n: isize) {
        // Bottom-up traversal not used (needs_postorder_traversal is false)
    }

    fn did_process_child(&self) -> isize {
        0
    }

    unsafe fn ensure_data(&self) -> AtomicRefMut<'_, style::data::ElementData> {
        let mut data = self.node().stylo_element_data.borrow_mut();
        if data.is_none() {
            *data = Some(Default::default());
        }
        AtomicRefMut::map(data, |d| d.as_mut().unwrap())
    }

    unsafe fn clear_data(&self) {
        *self.node().stylo_element_data.borrow_mut() = None;
    }

    fn has_data(&self) -> bool {
        self.node().stylo_element_data.borrow().is_some()
    }

    fn borrow_data(&self) -> Option<AtomicRef<'_, style::data::ElementData>> {
        let data = self.node().stylo_element_data.borrow();
        if data.is_some() {
            Some(AtomicRef::map(data, |d| d.as_ref().unwrap()))
        } else {
            None
        }
    }

    fn mutate_data(&self) -> Option<AtomicRefMut<'_, style::data::ElementData>> {
        let data = self.node().stylo_element_data.borrow_mut();
        if data.is_some() {
            Some(AtomicRefMut::map(data, |d| d.as_mut().unwrap()))
        } else {
            None
        }
    }

    fn skip_item_display_fixup(&self) -> bool {
        false
    }

    fn may_have_animations(&self) -> bool {
        false // No CSS animations in SSR
    }

    fn has_animations(&self, _context: &SharedStyleContext) -> bool {
        false
    }

    fn has_css_animations(
        &self,
        _context: &SharedStyleContext,
        _pseudo_element: Option<PseudoElement>,
    ) -> bool {
        false
    }

    fn has_css_transitions(
        &self,
        _context: &SharedStyleContext,
        _pseudo_element: Option<PseudoElement>,
    ) -> bool {
        false
    }

    fn animation_rule(
        &self,
        _context: &SharedStyleContext,
    ) -> Option<Arc<Locked<PropertyDeclarationBlock>>> {
        None
    }

    fn transition_rule(
        &self,
        _context: &SharedStyleContext,
    ) -> Option<Arc<Locked<PropertyDeclarationBlock>>> {
        None
    }

    fn shadow_root(&self) -> Option<<Self::ConcreteNode as TNode>::ConcreteShadowRoot> {
        None
    }

    fn containing_shadow(&self) -> Option<<Self::ConcreteNode as TNode>::ConcreteShadowRoot> {
        None
    }

    fn local_name(
        &self,
    ) -> &<SelectorImpl as selectors::SelectorImpl>::BorrowedLocalName {
        // SAFETY: We return a reference with the element's lifetime. The web_atoms::LocalName
        // is a string_cache::Atom which is interned and has 'static lifetime.
        // We leak a box here per element, which is acceptable for SSR (short-lived arena).
        // TODO: Cache this on the node to avoid repeated allocation.
        let name = self.elem_data().map(|d| &*d.name.local).unwrap_or("");
        let atom = web_atoms::LocalName::from(name);
        // Leak a box to extend lifetime. Fine for SSR render lifetime.
        Box::leak(Box::new(atom))
    }

    fn namespace(
        &self,
    ) -> &<SelectorImpl as selectors::SelectorImpl>::BorrowedNamespaceUrl {
        let ns = self.elem_data().map(|d| &*d.name.ns).unwrap_or("");
        let atom = web_atoms::Namespace::from(ns);
        Box::leak(Box::new(atom))
    }

    fn query_container_size(
        &self,
        _display: &style::values::computed::Display,
    ) -> euclid::default::Size2D<Option<app_units::Au>> {
        // No container queries support yet — return empty sizes.
        euclid::default::Size2D::new(None, None)
    }

    fn has_selector_flags(&self, flags: ElementSelectorFlags) -> bool {
        self.node().selector_flags.get().contains(flags)
    }

    fn relative_selector_search_direction(&self) -> ElementSelectorFlags {
        ElementSelectorFlags::empty()
    }

    fn lang_attr(&self) -> Option<style::selector_parser::AttrValue> {
        None
    }

    fn match_element_lang(
        &self,
        _override_lang: Option<Option<style::selector_parser::AttrValue>>,
        _value: &style::selector_parser::Lang,
    ) -> bool {
        false
    }

    fn is_html_document_body_element(&self) -> bool {
        let is_body = self
            .elem_data()
            .map(|d| &*d.name.local == "body" && d.name.ns == markup5ever::ns!(html))
            .unwrap_or(false);
        if !is_body {
            return false;
        }
        // Check if parent is the <html> root element
        if let Some(parent_id) = self.node().parent {
            let parent = StyloNode::new(self.arena, parent_id);
            return parent.is_root();
        }
        false
    }

    fn synthesize_presentational_hints_for_legacy_attributes<V>(
        &self,
        _visited_handling: VisitedHandlingMode,
        hints: &mut V,
    ) where
        V: Push<ApplicableDeclarationBlock>,
    {
        let Some(data) = self.elem_data() else {
            return;
        };
        let tag = &*data.name.local;

        // Handle `hidden` attribute → display: none
        if data.get_attribute("hidden").is_some() {
            use style::properties::{Importance, PropertyDeclaration};
            use style::rule_tree::CascadeLevel;
            use style::stylesheets::layer_rule::LayerOrder;
            use style::values::specified::Display;
            hints.push(ApplicableDeclarationBlock::from_declarations(
                Arc::new(self.arena.guard.wrap(PropertyDeclarationBlock::with_one(
                    PropertyDeclaration::Display(Display::None),
                    Importance::Normal,
                ))),
                CascadeLevel::PresHints,
                LayerOrder::root(),
            ));
        }

        // Handle width/height on elements that support them
        if matches!(
            tag,
            "img" | "canvas" | "video" | "table" | "td" | "th" | "col" | "colgroup"
                | "iframe" | "embed" | "object" | "input"
        ) {
            if let Some(width_val) = data.get_attribute("width") {
                if let Some(lp) = parse_size_attr(width_val) {
                    use style::properties::{Importance, PropertyDeclaration};
                    use style::rule_tree::CascadeLevel;
                    use style::stylesheets::layer_rule::LayerOrder;
                    use style::values::specified::Size;
                    use style::values::generics::NonNegative;
                    hints.push(ApplicableDeclarationBlock::from_declarations(
                        Arc::new(self.arena.guard.wrap(PropertyDeclarationBlock::with_one(
                            PropertyDeclaration::Width(Size::LengthPercentage(NonNegative(lp))),
                            Importance::Normal,
                        ))),
                        CascadeLevel::PresHints,
                        LayerOrder::root(),
                    ));
                }
            }
            if let Some(height_val) = data.get_attribute("height") {
                if let Some(lp) = parse_size_attr(height_val) {
                    use style::properties::{Importance, PropertyDeclaration};
                    use style::rule_tree::CascadeLevel;
                    use style::stylesheets::layer_rule::LayerOrder;
                    use style::values::specified::Size;
                    use style::values::generics::NonNegative;
                    hints.push(ApplicableDeclarationBlock::from_declarations(
                        Arc::new(self.arena.guard.wrap(PropertyDeclarationBlock::with_one(
                            PropertyDeclaration::Height(Size::LengthPercentage(NonNegative(lp))),
                            Importance::Normal,
                        ))),
                        CascadeLevel::PresHints,
                        LayerOrder::root(),
                    ));
                }
            }
        }
    }
}

/// Parse an HTML size attribute value (e.g. "100", "50%", "100px") into a LengthPercentage.
fn parse_size_attr(
    value: &str,
) -> Option<style::values::specified::LengthPercentage> {
    use style::values::specified::{AbsoluteLength, LengthPercentage, NoCalcLength};

    if let Some(pct) = value.strip_suffix('%') {
        let val: f32 = pct.parse().ok()?;
        return Some(LengthPercentage::Percentage(
            style::values::computed::Percentage(val / 100.0),
        ));
    }
    if let Some(px) = value.strip_suffix("px") {
        let val: f32 = px.parse().ok()?;
        return Some(LengthPercentage::Length(NoCalcLength::Absolute(
            AbsoluteLength::Px(val),
        )));
    }
    // Plain number → pixels
    let val: f32 = value.parse().ok()?;
    if val >= 0.0 {
        Some(LengthPercentage::Length(NoCalcLength::Absolute(
            AbsoluteLength::Px(val),
        )))
    } else {
        None
    }
}
