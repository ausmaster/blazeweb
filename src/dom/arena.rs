use slotmap::{new_key_type, SlotMap};

use super::node::{ElementData, NodeData};

new_key_type! {
    /// Generational key identifying a node in the arena.
    pub struct NodeId;
}

/// A single node in the DOM tree.
///
/// Tree structure uses a linked-list with parent/child/sibling pointers,
/// enabling O(1) insertBefore and removeChild.
#[derive(Debug, Clone)]
pub struct Node {
    pub data: NodeData,
    pub parent: Option<NodeId>,
    pub first_child: Option<NodeId>,
    pub last_child: Option<NodeId>,
    pub next_sibling: Option<NodeId>,
    pub prev_sibling: Option<NodeId>,
}

impl Node {
    pub fn new(data: NodeData) -> Self {
        Self {
            data,
            parent: None,
            first_child: None,
            last_child: None,
            next_sibling: None,
            prev_sibling: None,
        }
    }
}

/// Arena-based DOM tree. All nodes live in a SlotMap, referenced by NodeId.
pub struct Arena {
    pub nodes: SlotMap<NodeId, Node>,
    pub document: NodeId,
}

impl Arena {
    /// Create a new arena with a root Document node.
    pub fn new() -> Self {
        let mut nodes = SlotMap::with_key();
        let document = nodes.insert(Node::new(NodeData::Document));
        Self { nodes, document }
    }

    // -- Node creation --

    /// Create a new node in the arena (unattached to the tree).
    pub fn new_node(&mut self, data: NodeData) -> NodeId {
        self.nodes.insert(Node::new(data))
    }

    // -- Tree mutation --

    /// Append `child` as the last child of `parent`.
    pub fn append_child(&mut self, parent: NodeId, child: NodeId) {
        self.detach(child);
        self.nodes[child].parent = Some(parent);

        if let Some(last) = self.nodes[parent].last_child {
            self.nodes[last].next_sibling = Some(child);
            self.nodes[child].prev_sibling = Some(last);
            self.nodes[parent].last_child = Some(child);
        } else {
            self.nodes[parent].first_child = Some(child);
            self.nodes[parent].last_child = Some(child);
        }
    }

    /// Insert `new_child` before `reference` (which must be a child of some parent).
    pub fn insert_before(&mut self, reference: NodeId, new_child: NodeId) {
        let parent = self.nodes[reference]
            .parent
            .expect("insert_before: reference has no parent");
        self.detach(new_child);

        self.nodes[new_child].parent = Some(parent);
        self.nodes[new_child].next_sibling = Some(reference);

        if let Some(prev) = self.nodes[reference].prev_sibling {
            self.nodes[prev].next_sibling = Some(new_child);
            self.nodes[new_child].prev_sibling = Some(prev);
        } else {
            self.nodes[parent].first_child = Some(new_child);
        }
        self.nodes[reference].prev_sibling = Some(new_child);
    }

    /// Remove `child` from its parent, preserving the child's subtree.
    pub fn detach(&mut self, child: NodeId) {
        let parent = match self.nodes[child].parent {
            Some(p) => p,
            None => return,
        };

        let prev = self.nodes[child].prev_sibling;
        let next = self.nodes[child].next_sibling;

        // Fix previous sibling or parent.first_child
        if let Some(prev) = prev {
            self.nodes[prev].next_sibling = next;
        } else {
            self.nodes[parent].first_child = next;
        }

        // Fix next sibling or parent.last_child
        if let Some(next) = next {
            self.nodes[next].prev_sibling = prev;
        } else {
            self.nodes[parent].last_child = prev;
        }

        self.nodes[child].parent = None;
        self.nodes[child].prev_sibling = None;
        self.nodes[child].next_sibling = None;
    }

    /// Move all children of `from` to be children of `to`, appended in order.
    pub fn reparent_children(&mut self, from: NodeId, to: NodeId) {
        let mut child = self.nodes[from].first_child;
        while let Some(c) = child {
            let next = self.nodes[c].next_sibling;
            self.detach(c);
            self.append_child(to, c);
            child = next;
        }
    }

    // -- Tree queries --

    /// Iterate over the direct children of a node.
    pub fn children(&self, parent: NodeId) -> ChildrenIter<'_> {
        ChildrenIter {
            arena: self,
            next: self.nodes[parent].first_child,
        }
    }

    /// Get the element data for a node, if it's an Element.
    pub fn element_data(&self, id: NodeId) -> Option<&ElementData> {
        match &self.nodes[id].data {
            NodeData::Element(data) => Some(data),
            _ => None,
        }
    }

    /// Get mutable element data for a node, if it's an Element.
    pub fn element_data_mut(&mut self, id: NodeId) -> Option<&mut ElementData> {
        match &mut self.nodes[id].data {
            NodeData::Element(data) => Some(data),
            _ => None,
        }
    }

    /// Post-processing: populate `<selectedcontent>` elements inside
    /// customizable `<select>` with clones of the selected option's children.
    ///
    /// Per the WHATWG spec, `<selectedcontent>` inside a `<button>` inside a
    /// `<select>` should reflect the selected option's content. html5ever calls
    /// `maybe_clone_an_option_into_selectedcontent` during tree construction,
    /// but the tree may not be fully built at that point. We handle it here
    /// after parsing is complete.
    pub fn clone_selectedcontent(&mut self) {
        // 1. Find all <select> elements
        let selects: Vec<NodeId> = self.nodes.keys()
            .filter(|&id| {
                matches!(&self.nodes[id].data,
                    NodeData::Element(d) if &*d.name.local == "select")
            })
            .collect();

        for select in selects {
            // 2. Find the <selectedcontent> descendant (should be inside a <button>)
            let sc = match self.find_element(select, "selectedcontent") {
                Some(id) => id,
                None => continue,
            };

            // 3. Collect all <option> descendants of this select
            //    (but not options inside nested selects)
            let options = self.collect_options(select);
            if options.is_empty() {
                continue;
            }

            // 4. Find the selected option:
            //    - First option with `selected` attribute, OR
            //    - First option if none has `selected`
            let selected_option = options.iter().copied()
                .find(|&id| {
                    matches!(&self.nodes[id].data,
                        NodeData::Element(d) if d.get_attribute("selected").is_some())
                })
                .or(Some(options[0]));

            let selected_option = match selected_option {
                Some(id) => id,
                None => continue,
            };

            // 5. Clear selectedcontent and deep-clone option's children into it
            self.remove_all_children(sc);
            self.deep_clone_children(selected_option, sc);
        }
    }

    /// Collect all <option> elements that are descendants of `select`,
    /// skipping options inside nested <select> elements.
    pub fn collect_options(&self, select: NodeId) -> Vec<NodeId> {
        let mut result = Vec::new();
        self.collect_options_recursive(select, &mut result, true);
        result
    }

    fn collect_options_recursive(
        &self,
        node: NodeId,
        result: &mut Vec<NodeId>,
        is_root_select: bool,
    ) {
        for child in self.children(node) {
            if let NodeData::Element(data) = &self.nodes[child].data {
                // Skip nested <select> elements entirely
                if &*data.name.local == "select" && !is_root_select {
                    continue;
                }
                if &*data.name.local == "option" {
                    result.push(child);
                }
                self.collect_options_recursive(child, result, false);
            }
        }
    }

    /// Remove all children from a node.
    pub fn remove_all_children(&mut self, parent: NodeId) {
        while let Some(child) = self.nodes[parent].first_child {
            self.detach(child);
        }
    }

    /// Deep clone a node and all its descendants. Returns the new root NodeId.
    pub fn deep_clone(&mut self, source: NodeId) -> NodeId {
        let data = self.nodes[source].data.clone();
        let clone = self.new_node(data);

        // If it's an element with template_contents, clone those too
        if let NodeData::Element(ref clone_data) = self.nodes[clone].data {
            if let Some(template_contents) = clone_data.template_contents {
                let cloned_contents = self.deep_clone(template_contents);
                if let NodeData::Element(ref mut cd) = self.nodes[clone].data {
                    cd.template_contents = Some(cloned_contents);
                }
            }
        }

        // Clone all children
        let children: Vec<NodeId> = self.children(source).collect();
        for child in children {
            let cloned_child = self.deep_clone(child);
            self.append_child(clone, cloned_child);
        }
        clone
    }

    /// Deep clone all children of `source` and append them as children of `target`.
    pub fn deep_clone_children(&mut self, source: NodeId, target: NodeId) {
        let children: Vec<NodeId> = self.children(source).collect();
        for child in children {
            let cloned = self.deep_clone(child);
            self.append_child(target, cloned);
        }
    }

    /// Walk up from a node to find the nearest ancestor element with the given local name.
    pub fn ancestor_element(&self, node: NodeId, local_name: &str) -> Option<NodeId> {
        let mut current = self.nodes[node].parent;
        while let Some(id) = current {
            if let NodeData::Element(data) = &self.nodes[id].data {
                if &*data.name.local == local_name {
                    return Some(id);
                }
            }
            current = self.nodes[id].parent;
        }
        None
    }

    /// Find the first element with the given local tag name in a depth-first walk.
    pub fn find_element(&self, root: NodeId, local_name: &str) -> Option<NodeId> {
        if let NodeData::Element(data) = &self.nodes[root].data {
            if &*data.name.local == local_name {
                return Some(root);
            }
        }
        for child in self.children(root) {
            if let Some(found) = self.find_element(child, local_name) {
                return Some(found);
            }
        }
        None
    }
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}

/// Iterator over the direct children of a node.
pub struct ChildrenIter<'a> {
    arena: &'a Arena,
    next: Option<NodeId>,
}

impl<'a> Iterator for ChildrenIter<'a> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        let current = self.next?;
        self.next = self.arena.nodes[current].next_sibling;
        Some(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::node::NodeData;
    use markup5ever::{ns, Attribute, QualName};

    fn make_element(arena: &mut Arena, tag: &str) -> NodeId {
        let name = QualName::new(None, ns!(html), tag.into());
        let data = NodeData::Element(ElementData::new(name, vec![]));
        arena.new_node(data)
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
        let data = NodeData::Element(ElementData::new(name, attrs));
        arena.new_node(data)
    }

    fn make_text(arena: &mut Arena, text: &str) -> NodeId {
        arena.new_node(NodeData::Text(text.to_string()))
    }

    /// Collect all children of a node into a Vec.
    fn child_vec(arena: &Arena, parent: NodeId) -> Vec<NodeId> {
        arena.children(parent).collect()
    }

    // ─── Arena basics ───────────────────────────────────────

    #[test]
    fn new_arena_has_document_root() {
        let arena = Arena::new();
        assert!(matches!(arena.nodes[arena.document].data, NodeData::Document));
        assert_eq!(arena.nodes[arena.document].parent, None);
        assert_eq!(arena.nodes[arena.document].first_child, None);
    }

    #[test]
    fn new_node_is_detached() {
        let mut arena = Arena::new();
        let div = make_element(&mut arena, "div");
        assert_eq!(arena.nodes[div].parent, None);
        assert_eq!(arena.nodes[div].first_child, None);
        assert_eq!(arena.nodes[div].next_sibling, None);
        assert_eq!(arena.nodes[div].prev_sibling, None);
    }

    // ─── append_child ───────────────────────────────────────

    #[test]
    fn append_single_child() {
        let mut arena = Arena::new();
        let div = make_element(&mut arena, "div");
        arena.append_child(arena.document, div);

        assert_eq!(arena.nodes[div].parent, Some(arena.document));
        assert_eq!(arena.nodes[arena.document].first_child, Some(div));
        assert_eq!(arena.nodes[arena.document].last_child, Some(div));
        assert_eq!(arena.nodes[div].prev_sibling, None);
        assert_eq!(arena.nodes[div].next_sibling, None);
    }

    #[test]
    fn append_multiple_children_preserves_order() {
        let mut arena = Arena::new();
        let parent = make_element(&mut arena, "div");
        let a = make_element(&mut arena, "a");
        let b = make_element(&mut arena, "b");
        let c = make_element(&mut arena, "c");

        arena.append_child(arena.document, parent);
        arena.append_child(parent, a);
        arena.append_child(parent, b);
        arena.append_child(parent, c);

        assert_eq!(child_vec(&arena, parent), vec![a, b, c]);
        // first/last pointers
        assert_eq!(arena.nodes[parent].first_child, Some(a));
        assert_eq!(arena.nodes[parent].last_child, Some(c));
        // sibling links
        assert_eq!(arena.nodes[a].next_sibling, Some(b));
        assert_eq!(arena.nodes[b].next_sibling, Some(c));
        assert_eq!(arena.nodes[c].next_sibling, None);
        assert_eq!(arena.nodes[c].prev_sibling, Some(b));
        assert_eq!(arena.nodes[b].prev_sibling, Some(a));
        assert_eq!(arena.nodes[a].prev_sibling, None);
    }

    #[test]
    fn append_child_moves_from_old_parent() {
        let mut arena = Arena::new();
        let p1 = make_element(&mut arena, "div");
        let p2 = make_element(&mut arena, "span");
        let child = make_element(&mut arena, "a");

        arena.append_child(arena.document, p1);
        arena.append_child(arena.document, p2);
        arena.append_child(p1, child);
        assert_eq!(child_vec(&arena, p1), vec![child]);

        // Move child from p1 to p2
        arena.append_child(p2, child);
        assert_eq!(child_vec(&arena, p1), vec![]);
        assert_eq!(child_vec(&arena, p2), vec![child]);
        assert_eq!(arena.nodes[child].parent, Some(p2));
    }

    #[test]
    fn append_child_text_nodes() {
        let mut arena = Arena::new();
        let div = make_element(&mut arena, "div");
        let t1 = make_text(&mut arena, "hello ");
        let t2 = make_text(&mut arena, "world");

        arena.append_child(arena.document, div);
        arena.append_child(div, t1);
        arena.append_child(div, t2);

        assert_eq!(child_vec(&arena, div), vec![t1, t2]);
        assert!(matches!(&arena.nodes[t1].data, NodeData::Text(s) if s == "hello "));
        assert!(matches!(&arena.nodes[t2].data, NodeData::Text(s) if s == "world"));
    }

    // ─── insert_before ──────────────────────────────────────

    #[test]
    fn insert_before_first_child() {
        let mut arena = Arena::new();
        let parent = make_element(&mut arena, "div");
        let a = make_element(&mut arena, "a");
        let b = make_element(&mut arena, "b");

        arena.append_child(arena.document, parent);
        arena.append_child(parent, b);
        arena.insert_before(b, a);

        assert_eq!(child_vec(&arena, parent), vec![a, b]);
        assert_eq!(arena.nodes[parent].first_child, Some(a));
        assert_eq!(arena.nodes[parent].last_child, Some(b));
        assert_eq!(arena.nodes[a].prev_sibling, None);
        assert_eq!(arena.nodes[a].next_sibling, Some(b));
    }

    #[test]
    fn insert_before_middle() {
        let mut arena = Arena::new();
        let parent = make_element(&mut arena, "div");
        let a = make_element(&mut arena, "a");
        let b = make_element(&mut arena, "b");
        let c = make_element(&mut arena, "c");

        arena.append_child(arena.document, parent);
        arena.append_child(parent, a);
        arena.append_child(parent, c);
        arena.insert_before(c, b);

        assert_eq!(child_vec(&arena, parent), vec![a, b, c]);
        assert_eq!(arena.nodes[a].next_sibling, Some(b));
        assert_eq!(arena.nodes[b].prev_sibling, Some(a));
        assert_eq!(arena.nodes[b].next_sibling, Some(c));
        assert_eq!(arena.nodes[c].prev_sibling, Some(b));
    }

    #[test]
    fn insert_before_moves_from_old_parent() {
        let mut arena = Arena::new();
        let p1 = make_element(&mut arena, "div");
        let p2 = make_element(&mut arena, "span");
        let child = make_element(&mut arena, "a");
        let ref_node = make_element(&mut arena, "b");

        arena.append_child(arena.document, p1);
        arena.append_child(arena.document, p2);
        arena.append_child(p1, child);
        arena.append_child(p2, ref_node);

        arena.insert_before(ref_node, child);

        assert_eq!(child_vec(&arena, p1), vec![]);
        assert_eq!(child_vec(&arena, p2), vec![child, ref_node]);
    }

    // ─── detach ─────────────────────────────────────────────

    #[test]
    fn detach_only_child() {
        let mut arena = Arena::new();
        let parent = make_element(&mut arena, "div");
        let child = make_element(&mut arena, "a");

        arena.append_child(arena.document, parent);
        arena.append_child(parent, child);
        arena.detach(child);

        assert_eq!(child_vec(&arena, parent), vec![]);
        assert_eq!(arena.nodes[parent].first_child, None);
        assert_eq!(arena.nodes[parent].last_child, None);
        assert_eq!(arena.nodes[child].parent, None);
    }

    #[test]
    fn detach_first_child() {
        let mut arena = Arena::new();
        let parent = make_element(&mut arena, "div");
        let a = make_element(&mut arena, "a");
        let b = make_element(&mut arena, "b");

        arena.append_child(arena.document, parent);
        arena.append_child(parent, a);
        arena.append_child(parent, b);
        arena.detach(a);

        assert_eq!(child_vec(&arena, parent), vec![b]);
        assert_eq!(arena.nodes[parent].first_child, Some(b));
        assert_eq!(arena.nodes[b].prev_sibling, None);
    }

    #[test]
    fn detach_last_child() {
        let mut arena = Arena::new();
        let parent = make_element(&mut arena, "div");
        let a = make_element(&mut arena, "a");
        let b = make_element(&mut arena, "b");

        arena.append_child(arena.document, parent);
        arena.append_child(parent, a);
        arena.append_child(parent, b);
        arena.detach(b);

        assert_eq!(child_vec(&arena, parent), vec![a]);
        assert_eq!(arena.nodes[parent].last_child, Some(a));
        assert_eq!(arena.nodes[a].next_sibling, None);
    }

    #[test]
    fn detach_middle_child() {
        let mut arena = Arena::new();
        let parent = make_element(&mut arena, "div");
        let a = make_element(&mut arena, "a");
        let b = make_element(&mut arena, "b");
        let c = make_element(&mut arena, "c");

        arena.append_child(arena.document, parent);
        arena.append_child(parent, a);
        arena.append_child(parent, b);
        arena.append_child(parent, c);
        arena.detach(b);

        assert_eq!(child_vec(&arena, parent), vec![a, c]);
        assert_eq!(arena.nodes[a].next_sibling, Some(c));
        assert_eq!(arena.nodes[c].prev_sibling, Some(a));
        assert_eq!(arena.nodes[b].parent, None);
        assert_eq!(arena.nodes[b].prev_sibling, None);
        assert_eq!(arena.nodes[b].next_sibling, None);
    }

    #[test]
    fn detach_already_detached_is_noop() {
        let mut arena = Arena::new();
        let orphan = make_element(&mut arena, "div");
        // Should not panic
        arena.detach(orphan);
        assert_eq!(arena.nodes[orphan].parent, None);
    }

    #[test]
    fn detach_preserves_subtree() {
        let mut arena = Arena::new();
        let parent = make_element(&mut arena, "div");
        let child = make_element(&mut arena, "span");
        let grandchild = make_element(&mut arena, "a");

        arena.append_child(arena.document, parent);
        arena.append_child(parent, child);
        arena.append_child(child, grandchild);
        arena.detach(child);

        // child's subtree is intact
        assert_eq!(child_vec(&arena, child), vec![grandchild]);
        assert_eq!(arena.nodes[grandchild].parent, Some(child));
    }

    // ─── reparent_children ──────────────────────────────────

    #[test]
    fn reparent_children_moves_all() {
        let mut arena = Arena::new();
        let src = make_element(&mut arena, "div");
        let dst = make_element(&mut arena, "span");
        let a = make_element(&mut arena, "a");
        let b = make_element(&mut arena, "b");

        arena.append_child(arena.document, src);
        arena.append_child(arena.document, dst);
        arena.append_child(src, a);
        arena.append_child(src, b);

        arena.reparent_children(src, dst);

        assert_eq!(child_vec(&arena, src), vec![]);
        assert_eq!(child_vec(&arena, dst), vec![a, b]);
        assert_eq!(arena.nodes[a].parent, Some(dst));
        assert_eq!(arena.nodes[b].parent, Some(dst));
    }

    #[test]
    fn reparent_children_appends_to_existing() {
        let mut arena = Arena::new();
        let src = make_element(&mut arena, "div");
        let dst = make_element(&mut arena, "span");
        let existing = make_element(&mut arena, "p");
        let moved = make_element(&mut arena, "a");

        arena.append_child(arena.document, src);
        arena.append_child(arena.document, dst);
        arena.append_child(dst, existing);
        arena.append_child(src, moved);

        arena.reparent_children(src, dst);

        assert_eq!(child_vec(&arena, dst), vec![existing, moved]);
    }

    #[test]
    fn reparent_empty_is_noop() {
        let mut arena = Arena::new();
        let src = make_element(&mut arena, "div");
        let dst = make_element(&mut arena, "span");

        arena.append_child(arena.document, src);
        arena.append_child(arena.document, dst);

        arena.reparent_children(src, dst);
        assert_eq!(child_vec(&arena, dst), vec![]);
    }

    // ─── children iterator ──────────────────────────────────

    #[test]
    fn children_of_empty_node() {
        let arena = Arena::new();
        assert_eq!(child_vec(&arena, arena.document), vec![]);
    }

    #[test]
    fn children_does_not_recurse() {
        let mut arena = Arena::new();
        let parent = make_element(&mut arena, "div");
        let child = make_element(&mut arena, "span");
        let grandchild = make_element(&mut arena, "a");

        arena.append_child(arena.document, parent);
        arena.append_child(parent, child);
        arena.append_child(child, grandchild);

        // children() should only return direct children
        assert_eq!(child_vec(&arena, parent), vec![child]);
    }

    // ─── element_data / element_data_mut ────────────────────

    #[test]
    fn element_data_returns_some_for_element() {
        let mut arena = Arena::new();
        let div = make_element_with_attrs(&mut arena, "div", &[("id", "main"), ("class", "foo")]);
        let data = arena.element_data(div).unwrap();
        assert_eq!(&*data.name.local, "div");
        assert_eq!(data.get_attribute("id"), Some("main"));
        assert_eq!(data.get_attribute("class"), Some("foo"));
    }

    #[test]
    fn element_data_returns_none_for_text() {
        let mut arena = Arena::new();
        let text = make_text(&mut arena, "hello");
        assert!(arena.element_data(text).is_none());
    }

    #[test]
    fn element_data_returns_none_for_document() {
        let arena = Arena::new();
        assert!(arena.element_data(arena.document).is_none());
    }

    #[test]
    fn element_data_mut_can_modify_attrs() {
        let mut arena = Arena::new();
        let div = make_element_with_attrs(&mut arena, "div", &[("class", "old")]);

        arena.element_data_mut(div).unwrap().set_attribute("class", "new");
        assert_eq!(arena.element_data(div).unwrap().get_attribute("class"), Some("new"));

        arena.element_data_mut(div).unwrap().set_attribute("id", "test");
        assert_eq!(arena.element_data(div).unwrap().get_attribute("id"), Some("test"));
    }

    // ─── find_element ───────────────────────────────────────

    #[test]
    fn find_element_in_nested_tree() {
        let mut arena = Arena::new();
        let html = make_element(&mut arena, "html");
        let body = make_element(&mut arena, "body");
        let div = make_element(&mut arena, "div");
        let p = make_element(&mut arena, "p");

        arena.append_child(arena.document, html);
        arena.append_child(html, body);
        arena.append_child(body, div);
        arena.append_child(div, p);

        assert_eq!(arena.find_element(arena.document, "p"), Some(p));
        assert_eq!(arena.find_element(arena.document, "body"), Some(body));
    }

    #[test]
    fn find_element_returns_first_match() {
        let mut arena = Arena::new();
        let parent = make_element(&mut arena, "div");
        let p1 = make_element(&mut arena, "p");
        let p2 = make_element(&mut arena, "p");

        arena.append_child(arena.document, parent);
        arena.append_child(parent, p1);
        arena.append_child(parent, p2);

        assert_eq!(arena.find_element(arena.document, "p"), Some(p1));
    }

    #[test]
    fn find_element_not_found() {
        let mut arena = Arena::new();
        let div = make_element(&mut arena, "div");
        arena.append_child(arena.document, div);
        assert_eq!(arena.find_element(arena.document, "span"), None);
    }

    #[test]
    fn find_element_scoped_to_subtree() {
        let mut arena = Arena::new();
        let parent = make_element(&mut arena, "div");
        let child1 = make_element(&mut arena, "section");
        let child2 = make_element(&mut arena, "aside");
        let target = make_element(&mut arena, "a");

        arena.append_child(arena.document, parent);
        arena.append_child(parent, child1);
        arena.append_child(parent, child2);
        arena.append_child(child2, target);

        // Search from child1 should not find target under child2
        assert_eq!(arena.find_element(child1, "a"), None);
        assert_eq!(arena.find_element(child2, "a"), Some(target));
    }

    // ─── Attribute operations on ElementData ────────────────

    #[test]
    fn get_attribute_missing() {
        let mut arena = Arena::new();
        let div = make_element(&mut arena, "div");
        assert_eq!(arena.element_data(div).unwrap().get_attribute("id"), None);
    }

    #[test]
    fn set_attribute_creates_new() {
        let mut arena = Arena::new();
        let div = make_element(&mut arena, "div");
        arena.element_data_mut(div).unwrap().set_attribute("id", "test");
        assert_eq!(arena.element_data(div).unwrap().get_attribute("id"), Some("test"));
    }

    #[test]
    fn set_attribute_overwrites_existing() {
        let mut arena = Arena::new();
        let div = make_element_with_attrs(&mut arena, "div", &[("id", "old")]);
        arena.element_data_mut(div).unwrap().set_attribute("id", "new");
        assert_eq!(arena.element_data(div).unwrap().get_attribute("id"), Some("new"));
        // Should not duplicate the attribute
        assert_eq!(arena.element_data(div).unwrap().attrs.len(), 1);
    }

    #[test]
    fn remove_attribute_existing() {
        let mut arena = Arena::new();
        let div = make_element_with_attrs(&mut arena, "div", &[("id", "x"), ("class", "y")]);
        let removed = arena.element_data_mut(div).unwrap().remove_attribute("id");
        assert!(removed);
        assert_eq!(arena.element_data(div).unwrap().get_attribute("id"), None);
        assert_eq!(arena.element_data(div).unwrap().get_attribute("class"), Some("y"));
    }

    #[test]
    fn remove_attribute_nonexistent() {
        let mut arena = Arena::new();
        let div = make_element(&mut arena, "div");
        let removed = arena.element_data_mut(div).unwrap().remove_attribute("nope");
        assert!(!removed);
    }

    // ─── Complex tree operations ────────────────────────────

    #[test]
    fn build_realistic_tree_and_verify_structure() {
        // Build: <html><head><title></title></head><body><div><p></p><p></p></div></body></html>
        let mut arena = Arena::new();
        let html = make_element(&mut arena, "html");
        let head = make_element(&mut arena, "head");
        let title = make_element(&mut arena, "title");
        let body = make_element(&mut arena, "body");
        let div = make_element(&mut arena, "div");
        let p1 = make_element(&mut arena, "p");
        let p2 = make_element(&mut arena, "p");

        arena.append_child(arena.document, html);
        arena.append_child(html, head);
        arena.append_child(head, title);
        arena.append_child(html, body);
        arena.append_child(body, div);
        arena.append_child(div, p1);
        arena.append_child(div, p2);

        // Verify entire structure
        assert_eq!(child_vec(&arena, arena.document), vec![html]);
        assert_eq!(child_vec(&arena, html), vec![head, body]);
        assert_eq!(child_vec(&arena, head), vec![title]);
        assert_eq!(child_vec(&arena, body), vec![div]);
        assert_eq!(child_vec(&arena, div), vec![p1, p2]);
        assert_eq!(child_vec(&arena, p1), vec![]);

        // Verify parent chain
        assert_eq!(arena.nodes[p1].parent, Some(div));
        assert_eq!(arena.nodes[div].parent, Some(body));
        assert_eq!(arena.nodes[body].parent, Some(html));
        assert_eq!(arena.nodes[html].parent, Some(arena.document));
    }

    #[test]
    fn move_subtree_between_parents() {
        let mut arena = Arena::new();
        let src = make_element(&mut arena, "div");
        let dst = make_element(&mut arena, "span");
        let subtree_root = make_element(&mut arena, "section");
        let deep_child = make_element(&mut arena, "a");

        arena.append_child(arena.document, src);
        arena.append_child(arena.document, dst);
        arena.append_child(src, subtree_root);
        arena.append_child(subtree_root, deep_child);

        // Move subtree_root (with deep_child) from src to dst
        arena.append_child(dst, subtree_root);

        assert_eq!(child_vec(&arena, src), vec![]);
        assert_eq!(child_vec(&arena, dst), vec![subtree_root]);
        assert_eq!(child_vec(&arena, subtree_root), vec![deep_child]);
        assert_eq!(arena.nodes[subtree_root].parent, Some(dst));
        assert_eq!(arena.nodes[deep_child].parent, Some(subtree_root));
    }

    #[test]
    fn reorder_children_via_detach_and_append() {
        let mut arena = Arena::new();
        let parent = make_element(&mut arena, "div");
        let a = make_element(&mut arena, "a");
        let b = make_element(&mut arena, "b");
        let c = make_element(&mut arena, "c");

        arena.append_child(arena.document, parent);
        arena.append_child(parent, a);
        arena.append_child(parent, b);
        arena.append_child(parent, c);

        // Move 'a' to the end: [b, c, a]
        arena.detach(a);
        arena.append_child(parent, a);

        assert_eq!(child_vec(&arena, parent), vec![b, c, a]);
        assert_eq!(arena.nodes[parent].first_child, Some(b));
        assert_eq!(arena.nodes[parent].last_child, Some(a));
    }
}
