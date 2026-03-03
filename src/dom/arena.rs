use slotmap::{new_key_type, SlotMap};

use super::node::{ElementData, NodeData};

// ─── Node flags (ported from Servo's NodeFlags) ──────────────────────────────

/// Bitflags for node state, enabling O(1) checks for common properties.
/// Ported from Servo's `NodeFlags` in `components/script/dom/node.rs`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NodeFlags(u16);

impl NodeFlags {
    /// The node is connected to the document tree (reachable from the root Document).
    pub const IS_CONNECTED: u16 = 1 << 0;

    pub fn is_connected(self) -> bool {
        self.0 & Self::IS_CONNECTED != 0
    }

    pub fn set_connected(&mut self, connected: bool) {
        if connected {
            self.0 |= Self::IS_CONNECTED;
        } else {
            self.0 &= !Self::IS_CONNECTED;
        }
    }
}

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
    pub flags: NodeFlags,
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
            flags: NodeFlags::default(),
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
        let mut doc_node = Node::new(NodeData::Document);
        doc_node.flags.set_connected(true); // Document is always connected
        let document = nodes.insert(doc_node);
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

        // Propagate connectivity
        let parent_connected = self.nodes[parent].flags.is_connected();
        if parent_connected {
            self.set_connected_recursive(child, true);
        }
    }

    /// Insert `new_child` before `reference`.
    /// If `reference` has no parent, this is a no-op (per DOM spec,
    /// ChildNode methods on parentless nodes are no-ops).
    pub fn insert_before(&mut self, reference: NodeId, new_child: NodeId) {
        let parent = match self.nodes[reference].parent {
            Some(p) => p,
            None => return,
        };
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

        // Propagate connectivity
        let parent_connected = self.nodes[parent].flags.is_connected();
        if parent_connected {
            self.set_connected_recursive(new_child, true);
        }
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

        // Detached nodes are no longer connected
        if self.nodes[child].flags.is_connected() {
            self.set_connected_recursive(child, false);
        }
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

    /// Recursively set IS_CONNECTED flag on a node and all its descendants.
    pub fn set_connected_recursive(&mut self, node: NodeId, connected: bool) {
        self.nodes[node].flags.set_connected(connected);
        let mut child = self.nodes[node].first_child;
        while let Some(c) = child {
            self.set_connected_recursive(c, connected);
            child = self.nodes[c].next_sibling;
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

    // -- Validation (WHATWG DOM §4.4) ─────────────────────────────────────

    /// Check if `ancestor` is an inclusive ancestor of `node`.
    /// (i.e., ancestor == node, or ancestor is an ancestor of node.)
    pub fn is_inclusive_ancestor_of(&self, ancestor: NodeId, node: NodeId) -> bool {
        let mut current = Some(node);
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = self.nodes[id].parent;
        }
        false
    }

    /// Count element children of a node.
    fn count_element_children(&self, parent: NodeId) -> usize {
        self.children(parent)
            .filter(|&c| matches!(&self.nodes[c].data, NodeData::Element(_)))
            .count()
    }

    /// Check if a node has a Doctype child.
    fn has_doctype_child(&self, parent: NodeId) -> bool {
        self.children(parent)
            .any(|c| matches!(&self.nodes[c].data, NodeData::Doctype { .. }))
    }

    /// Check if any child is a Doctype that is NOT the given `exclude` node.
    fn has_other_doctype_child(&self, parent: NodeId, exclude: NodeId) -> bool {
        self.children(parent)
            .any(|c| c != exclude && matches!(&self.nodes[c].data, NodeData::Doctype { .. }))
    }

    /// Check if any element child exists that is NOT the given `exclude` node.
    fn has_other_element_child(&self, parent: NodeId, exclude: NodeId) -> bool {
        self.children(parent)
            .any(|c| c != exclude && matches!(&self.nodes[c].data, NodeData::Element(_)))
    }

    /// Check if a doctype exists among the inclusive following siblings of `node`.
    fn doctype_following_or_is(&self, node: NodeId) -> bool {
        let mut current = Some(node);
        while let Some(id) = current {
            if matches!(&self.nodes[id].data, NodeData::Doctype { .. }) {
                return true;
            }
            current = self.nodes[id].next_sibling;
        }
        false
    }

    /// Check if an element exists before `node` among its parent's children.
    fn element_preceding(&self, parent: NodeId, node: NodeId) -> bool {
        for child in self.children(parent) {
            if child == node {
                return false;
            }
            if matches!(&self.nodes[child].data, NodeData::Element(_)) {
                return true;
            }
        }
        false
    }

    /// WHATWG DOM §4.4 ensure pre-insertion validity.
    /// Ported from Servo's node.rs ensure_pre_insertion_validity.
    ///
    /// Returns Ok(()) if the insertion is valid, or Err(DomValidationError).
    pub fn ensure_pre_insertion_validity(
        &self,
        node: NodeId,
        parent: NodeId,
        child: Option<NodeId>,
    ) -> Result<(), DomValidationError> {
        // Step 1: Parent must be Document, DocumentFragment, or Element.
        match &self.nodes[parent].data {
            NodeData::Document | NodeData::DocumentFragment | NodeData::Element(_) => {}
            _ => return Err(DomValidationError::HierarchyRequest),
        }

        // Step 2: node must not be an inclusive ancestor of parent.
        if self.is_inclusive_ancestor_of(node, parent) {
            return Err(DomValidationError::HierarchyRequest);
        }

        // Step 3: If child is non-null, it must be a child of parent.
        if let Some(child_id) = child {
            if self.nodes[child_id].parent != Some(parent) {
                return Err(DomValidationError::NotFound);
            }
        }

        // Step 4+5: node type restrictions.
        match &self.nodes[node].data {
            NodeData::Text(_) => {
                // Text cannot be child of Document
                if matches!(&self.nodes[parent].data, NodeData::Document) {
                    return Err(DomValidationError::HierarchyRequest);
                }
            }
            NodeData::Doctype { .. } => {
                // Doctype can only be child of Document
                if !matches!(&self.nodes[parent].data, NodeData::Document) {
                    return Err(DomValidationError::HierarchyRequest);
                }
            }
            NodeData::Element(_) | NodeData::Comment(_) | NodeData::DocumentFragment => {
                // These are always OK (subject to Step 6 Document constraints below)
            }
            NodeData::Document => {
                // Cannot insert a Document node
                return Err(DomValidationError::HierarchyRequest);
            }
        }

        // Step 6: If parent is a Document, additional constraints.
        if matches!(&self.nodes[parent].data, NodeData::Document) {
            match &self.nodes[node].data {
                NodeData::Element(_) => {
                    // Document can have at most one Element child.
                    if self.count_element_children(parent) > 0 {
                        return Err(DomValidationError::HierarchyRequest);
                    }
                    // If child is non-null and a doctype follows child, reject.
                    if let Some(child_id) = child {
                        if self.doctype_following_or_is(child_id) {
                            return Err(DomValidationError::HierarchyRequest);
                        }
                    }
                }
                NodeData::Doctype { .. } => {
                    // Document can have at most one Doctype child.
                    if self.has_doctype_child(parent) {
                        return Err(DomValidationError::HierarchyRequest);
                    }
                    // If child is non-null, no element may precede child.
                    if let Some(child_id) = child {
                        if self.element_preceding(parent, child_id) {
                            return Err(DomValidationError::HierarchyRequest);
                        }
                    } else {
                        // child is null (appending) — no element child may exist.
                        if self.count_element_children(parent) > 0 {
                            return Err(DomValidationError::HierarchyRequest);
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Validation for replaceChild — similar to pre-insertion but accounts
    /// for the node being replaced.
    pub fn ensure_replace_validity(
        &self,
        node: NodeId,
        parent: NodeId,
        child: NodeId,
    ) -> Result<(), DomValidationError> {
        // Step 1: Parent must be Document, DocumentFragment, or Element.
        match &self.nodes[parent].data {
            NodeData::Document | NodeData::DocumentFragment | NodeData::Element(_) => {}
            _ => return Err(DomValidationError::HierarchyRequest),
        }

        // Step 2: node must not be an inclusive ancestor of parent.
        if self.is_inclusive_ancestor_of(node, parent) {
            return Err(DomValidationError::HierarchyRequest);
        }

        // Step 3: child must be a child of parent.
        if self.nodes[child].parent != Some(parent) {
            return Err(DomValidationError::NotFound);
        }

        // Step 4+5: node type restrictions (same as pre-insert).
        match &self.nodes[node].data {
            NodeData::Text(_) => {
                if matches!(&self.nodes[parent].data, NodeData::Document) {
                    return Err(DomValidationError::HierarchyRequest);
                }
            }
            NodeData::Doctype { .. } => {
                if !matches!(&self.nodes[parent].data, NodeData::Document) {
                    return Err(DomValidationError::HierarchyRequest);
                }
            }
            NodeData::Element(_) | NodeData::Comment(_) | NodeData::DocumentFragment => {}
            NodeData::Document => {
                return Err(DomValidationError::HierarchyRequest);
            }
        }

        // Step 6: If parent is a Document, additional constraints.
        if matches!(&self.nodes[parent].data, NodeData::Document) {
            match &self.nodes[node].data {
                NodeData::Element(_) => {
                    // No OTHER element child may exist (the one being replaced doesn't count).
                    if self.has_other_element_child(parent, child) {
                        return Err(DomValidationError::HierarchyRequest);
                    }
                    // No doctype may follow child.
                    if let Some(next) = self.nodes[child].next_sibling {
                        if self.doctype_following_or_is(next) {
                            return Err(DomValidationError::HierarchyRequest);
                        }
                    }
                }
                NodeData::Doctype { .. } => {
                    // No OTHER doctype child may exist.
                    if self.has_other_doctype_child(parent, child) {
                        return Err(DomValidationError::HierarchyRequest);
                    }
                    // No element may precede the child being replaced.
                    if self.element_preceding(parent, child) {
                        return Err(DomValidationError::HierarchyRequest);
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }
}

/// DOM validation errors, ported from Servo's DOMException.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomValidationError {
    /// The operation would result in an invalid DOM hierarchy.
    HierarchyRequest,
    /// The referenced child node was not found.
    NotFound,
}

impl std::fmt::Display for DomValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DomValidationError::HierarchyRequest => {
                write!(f, "HierarchyRequestError: The operation would yield an incorrect node tree.")
            }
            DomValidationError::NotFound => {
                write!(f, "NotFoundError: The object can not be found here.")
            }
        }
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
#[path = "arena_tests.rs"]
mod tests;
