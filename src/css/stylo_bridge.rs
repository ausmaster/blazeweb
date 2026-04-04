//! Stylo trait bridge — implements TDocument, TNode, TElement for our Arena-based DOM.
//!
//! Stylo's style sharing cache requires `size_of::<E>() == size_of::<usize>()`.
//! Since our StyloNode needs both an arena reference and a NodeId, we store the
//! arena pointer in a thread-local and make StyloNode just a NodeId wrapper.
//! The thread-local is set before traversal and cleared after.

use std::cell::Cell;
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

// ─── Thread-local arena pointer ──────────────────────────────────────────
//
// Stylo's SharingCache requires size_of::<TElement>() == size_of::<usize>(),
// so StyloNode can only hold a NodeId (8 bytes). The arena pointer is stored
// in a thread-local, set before traversal and cleared after.
//
// We use sequential traversal (no rayon pool) so only one thread ever accesses
// the TLS during style resolution. Each test thread gets its own TLS naturally.

thread_local! {
    static ARENA_PTR: Cell<*const Arena> = const { Cell::new(std::ptr::null()) };
}

/// Set the thread-local arena pointer. Must be called before any StyloNode
/// methods and cleared after style resolution.
///
/// # Safety
/// The caller must ensure the Arena reference outlives all StyloNode usage.
pub unsafe fn set_arena(arena: &Arena) {
    ARENA_PTR.with(|p| p.set(arena as *const Arena));
}

/// Clear the thread-local arena pointer.
pub fn clear_arena() {
    ARENA_PTR.with(|p| p.set(std::ptr::null()));
}

#[inline]
fn arena<'a>() -> &'a Arena {
    ARENA_PTR.with(|p| {
        let ptr = p.get();
        assert!(!ptr.is_null(), "ARENA_PTR not set — call set_arena() before style resolution");
        unsafe { &*ptr }
    })
}

// ─── StyloNode wrapper ──────────────────────────────────────────────────────

/// Pointer-sized handle into our Arena for Stylo's trait system.
///
/// Must be exactly `usize`-sized to satisfy Stylo's `SharingCache` size assertion.
/// The arena pointer is stored in a thread-local, so this only holds a NodeId.
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct StyloNode {
    pub id: NodeId,
}

// Compile-time size assertion: StyloNode must be usize-sized for Stylo's SharingCache.
const _: () = assert!(std::mem::size_of::<StyloNode>() == std::mem::size_of::<usize>());

impl StyloNode {
    pub fn new(id: NodeId) -> Self {
        Self { id }
    }

    /// Create a StyloNode and also set the thread-local arena pointer.
    /// Convenience for the entry point to style resolution.
    ///
    /// # Safety
    /// The caller must ensure the Arena reference outlives all StyloNode usage.
    pub unsafe fn with_arena(arena: &Arena, id: NodeId) -> Self {
        // SAFETY: caller guarantees arena outlives all StyloNode usage
        unsafe { set_arena(arena) };
        Self { id }
    }

    #[inline]
    fn node(&self) -> &crate::dom::arena::Node {
        &arena().nodes[self.id]
    }

    fn elem_data(&self) -> Option<&crate::dom::node::ElementData> {
        arena().element_data(self.id)
    }
}

impl std::fmt::Debug for StyloNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StyloNode({:?})", self.id)
    }
}

impl PartialEq for StyloNode {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for StyloNode {}

impl Hash for StyloNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

// ─── NodeInfo ────────────────────────────────────────────────────────────────

impl NodeInfo for StyloNode {
    fn is_element(&self) -> bool {
        matches!(&self.node().data, NodeData::Element(_))
    }

    fn is_text_node(&self) -> bool {
        matches!(&self.node().data, NodeData::Text(_))
    }
}

// ─── TDocument ───────────────────────────────────────────────────────────────

impl TDocument for StyloNode {
    type ConcreteNode = StyloNode;

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
        &arena().guard
    }
}

// ─── TShadowRoot ─────────────────────────────────────────────────────────────

impl TShadowRoot for StyloNode {
    type ConcreteNode = StyloNode;

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

impl TNode for StyloNode {
    type ConcreteElement = StyloNode;
    type ConcreteDocument = StyloNode;
    type ConcreteShadowRoot = StyloNode;

    fn parent_node(&self) -> Option<Self> {
        self.node().parent.map(StyloNode::new)
    }

    fn first_child(&self) -> Option<Self> {
        self.node().first_child.map(StyloNode::new)
    }

    fn last_child(&self) -> Option<Self> {
        self.node().last_child.map(StyloNode::new)
    }

    fn prev_sibling(&self) -> Option<Self> {
        self.node().prev_sibling.map(StyloNode::new)
    }

    fn next_sibling(&self) -> Option<Self> {
        self.node().next_sibling.map(StyloNode::new)
    }

    fn owner_doc(&self) -> Self::ConcreteDocument {
        StyloNode::new(arena().document)
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

pub struct StyloChildIter {
    current: Option<NodeId>,
}

impl Iterator for StyloChildIter {
    type Item = StyloNode;

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.current?;
        self.current = arena().nodes[id].next_sibling;
        Some(StyloNode::new(id))
    }
}

// ─── AttributeProvider ───────────────────────────────────────────────────────

impl AttributeProvider for StyloNode {
    fn get_attr(&self, attr: &LocalName) -> Option<String> {
        let elem = self.elem_data()?;
        let attr_name: &str = attr.as_ref();
        elem.get_attribute(attr_name).map(|s| s.to_string())
    }
}

// ─── selectors::Element (Stylo's SelectorImpl) ──────────────────────────────

impl SelectorsElement for StyloNode {
    type Impl = SelectorImpl;

    fn opaque(&self) -> OpaqueElement {
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
        let a = arena();
        let mut current = self.node().prev_sibling;
        while let Some(id) = current {
            if matches!(&a.nodes[id].data, NodeData::Element(_)) {
                return Some(StyloNode::new(id));
            }
            current = a.nodes[id].prev_sibling;
        }
        None
    }

    fn next_sibling_element(&self) -> Option<Self> {
        let a = arena();
        let mut current = self.node().next_sibling;
        while let Some(id) = current {
            if matches!(&a.nodes[id].data, NodeData::Element(_)) {
                return Some(StyloNode::new(id));
            }
            current = a.nodes[id].next_sibling;
        }
        None
    }

    fn first_element_child(&self) -> Option<Self> {
        let a = arena();
        let mut current = self.node().first_child;
        while let Some(id) = current {
            if matches!(&a.nodes[id].data, NodeData::Element(_)) {
                return Some(StyloNode::new(id));
            }
            current = a.nodes[id].next_sibling;
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
            .map(|d| *d.name.local == **local_name)
            .unwrap_or(false)
    }

    fn has_namespace(&self, ns: &web_atoms::Namespace) -> bool {
        self.elem_data()
            .map(|d| *d.name.ns == **ns)
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
            if &*attr.name.local != local_str {
                continue;
            }
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
                self.elem_data()
                    .map(|d| d.name.ns == markup5ever::ns!(html))
                    .unwrap_or(false)
            }
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
        let a = arena();
        let self_flags = flags.for_self();
        if !self_flags.is_empty() {
            let node = self.node();
            node.selector_flags
                .set(node.selector_flags.get() | self_flags);
        }
        let parent_flags = flags.for_parent();
        if !parent_flags.is_empty()
            && let Some(parent_id) = self.node().parent
        {
            let parent = &a.nodes[parent_id];
            parent
                .selector_flags
                .set(parent.selector_flags.get() | parent_flags);
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
        let a = arena();
        let mut child = self.node().first_child;
        while let Some(id) = child {
            match &a.nodes[id].data {
                NodeData::Element(_) => return false,
                NodeData::Text(t) if !t.is_empty() => return false,
                _ => {}
            }
            child = a.nodes[id].next_sibling;
        }
        true
    }

    fn is_root(&self) -> bool {
        self.node()
            .parent
            .map(|pid| matches!(&arena().nodes[pid].data, NodeData::Document))
            .unwrap_or(false)
    }

    fn add_element_unique_hashes(&self, _filter: &mut selectors::bloom::BloomFilter) -> bool {
        false
    }
}

// ─── TElement ────────────────────────────────────────────────────────────────

impl TElement for StyloNode {
    type ConcreteNode = StyloNode;
    type TraversalChildrenIterator = StyloChildIter;

    fn as_node(&self) -> Self::ConcreteNode {
        *self
    }

    fn traversal_children(&self) -> LayoutIterator<Self::TraversalChildrenIterator> {
        LayoutIterator(StyloChildIter {
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
        self.node()
            .parsed_style_attribute
            .as_ref()
            .map(|arc| arc.borrow_arc())
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
        self.node().cached_atom_id.as_ref()
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
        false
    }

    fn handled_snapshot(&self) -> bool {
        true
    }

    unsafe fn set_handled_snapshot(&self) {}

    unsafe fn set_dirty_descendants(&self) {
        let a = arena();
        self.node()
            .dirty_descendants
            .store(true, Ordering::Relaxed);
        let mut current = self.node().parent;
        while let Some(id) = current {
            let node = &a.nodes[id];
            if node.dirty_descendants.load(Ordering::Relaxed) {
                break;
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

    fn store_children_to_process(&self, _n: isize) {}

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
        false
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
        self.node()
            .cached_atom_local_name
            .as_ref()
            .expect("cached_atom_local_name not set — prepare_for_stylo() must run before style resolution")
    }

    fn namespace(
        &self,
    ) -> &<SelectorImpl as selectors::SelectorImpl>::BorrowedNamespaceUrl {
        self.node()
            .cached_atom_namespace
            .as_ref()
            .expect("cached_atom_namespace not set — prepare_for_stylo() must run before style resolution")
    }

    fn query_container_size(
        &self,
        _display: &style::values::computed::Display,
    ) -> euclid::default::Size2D<Option<app_units::Au>> {
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
        if let Some(parent_id) = self.node().parent {
            let parent = StyloNode::new(parent_id);
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
        let a = arena();
        let tag = &*data.name.local;

        // hidden → display: none
        if data.get_attribute("hidden").is_some() {
            log::trace!("Presentational hint: hidden attr on <{}>", tag);
            use style::properties::{Importance, PropertyDeclaration};
            use style::rule_tree::CascadeLevel;
            use style::stylesheets::layer_rule::LayerOrder;
            use style::values::specified::Display;
            hints.push(ApplicableDeclarationBlock::from_declarations(
                Arc::new(a.guard.wrap(PropertyDeclarationBlock::with_one(
                    PropertyDeclaration::Display(Display::None),
                    Importance::Normal,
                ))),
                CascadeLevel::PresHints,
                LayerOrder::root(),
            ));
        }

        if matches!(
            tag,
            "img" | "canvas" | "video" | "table" | "td" | "th" | "col" | "colgroup"
                | "iframe" | "embed" | "object" | "input"
        ) {
            if let Some(width_val) = data.get_attribute("width")
                && let Some(lp) = parse_size_attr(width_val)
            {
                use style::properties::{Importance, PropertyDeclaration};
                use style::rule_tree::CascadeLevel;
                use style::stylesheets::layer_rule::LayerOrder;
                use style::values::specified::Size;
                use style::values::generics::NonNegative;
                hints.push(ApplicableDeclarationBlock::from_declarations(
                    Arc::new(a.guard.wrap(PropertyDeclarationBlock::with_one(
                        PropertyDeclaration::Width(Size::LengthPercentage(NonNegative(lp))),
                        Importance::Normal,
                    ))),
                    CascadeLevel::PresHints,
                    LayerOrder::root(),
                ));
            }
            if let Some(height_val) = data.get_attribute("height")
                && let Some(lp) = parse_size_attr(height_val)
            {
                use style::properties::{Importance, PropertyDeclaration};
                use style::rule_tree::CascadeLevel;
                use style::stylesheets::layer_rule::LayerOrder;
                use style::values::specified::Size;
                use style::values::generics::NonNegative;
                hints.push(ApplicableDeclarationBlock::from_declarations(
                    Arc::new(a.guard.wrap(PropertyDeclarationBlock::with_one(
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

/// Parse an HTML size attribute value (e.g. "100", "50%", "100px") into a LengthPercentage.
pub(crate) fn parse_size_attr(
    value: &str,
) -> Option<style::values::specified::LengthPercentage> {
    use style::values::specified::{AbsoluteLength, LengthPercentage, NoCalcLength};

    if let Some(pct) = value.strip_suffix('%') {
        let val: f32 = pct.parse().ok()?;
        if val < 0.0 {
            return None;
        }
        return Some(LengthPercentage::Percentage(
            style::values::computed::Percentage(val / 100.0),
        ));
    }
    if let Some(px) = value.strip_suffix("px") {
        let val: f32 = px.parse().ok()?;
        if val < 0.0 {
            return None;
        }
        return Some(LengthPercentage::Length(NoCalcLength::Absolute(
            AbsoluteLength::Px(val),
        )));
    }
    let val: f32 = value.parse().ok()?;
    if val >= 0.0 {
        Some(LengthPercentage::Length(NoCalcLength::Absolute(
            AbsoluteLength::Px(val),
        )))
    } else {
        None
    }
}

#[cfg(test)]
#[path = "stylo_bridge_tests.rs"]
mod tests;
