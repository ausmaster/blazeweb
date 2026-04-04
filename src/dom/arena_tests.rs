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

    // ─── deep_clone (importNode deep) ────────────────────────

    fn make_comment(arena: &mut Arena, text: &str) -> NodeId {
        arena.new_node(NodeData::Comment(text.to_string()))
    }

    #[test]
    fn deep_clone_element_with_children() {
        let mut arena = Arena::new();
        let div = make_element(&mut arena, "div");
        let span = make_element(&mut arena, "span");
        let p = make_element(&mut arena, "p");
        arena.append_child(div, span);
        arena.append_child(div, p);

        let clone = arena.deep_clone(div);

        // Clone is a different node
        assert_ne!(clone, div);
        // Clone has same number of children
        let clone_children = child_vec(&arena, clone);
        assert_eq!(clone_children.len(), 2);
        // Children are different nodes from originals
        assert_ne!(clone_children[0], span);
        assert_ne!(clone_children[1], p);
        // Children are elements with correct tags
        if let NodeData::Element(ref data) = arena.nodes[clone_children[0]].data {
            assert_eq!(&*data.name.local, "span");
        } else {
            panic!("expected element");
        }
        if let NodeData::Element(ref data) = arena.nodes[clone_children[1]].data {
            assert_eq!(&*data.name.local, "p");
        } else {
            panic!("expected element");
        }
    }

    #[test]
    fn deep_clone_preserves_attributes() {
        let mut arena = Arena::new();
        let div = make_element_with_attrs(&mut arena, "div", &[("id", "test"), ("class", "foo bar")]);
        let clone = arena.deep_clone(div);

        if let NodeData::Element(ref data) = arena.nodes[clone].data {
            assert_eq!(data.attrs.len(), 2);
            assert_eq!(&*data.attrs[0].value, "test");
            assert_eq!(&*data.attrs[1].value, "foo bar");
        } else {
            panic!("expected element");
        }
    }

    #[test]
    fn deep_clone_nested_structure() {
        let mut arena = Arena::new();
        let div = make_element(&mut arena, "div");
        let span = make_element(&mut arena, "span");
        let text = make_text(&mut arena, "hello");
        arena.append_child(div, span);
        arena.append_child(span, text);

        let clone = arena.deep_clone(div);
        let clone_children = child_vec(&arena, clone);
        assert_eq!(clone_children.len(), 1);

        let inner_children = child_vec(&arena, clone_children[0]);
        assert_eq!(inner_children.len(), 1);
        if let NodeData::Text(ref s) = arena.nodes[inner_children[0]].data {
            assert_eq!(s, "hello");
        } else {
            panic!("expected text node");
        }
    }

    #[test]
    fn deep_clone_comment_preserves_text() {
        let mut arena = Arena::new();
        let comment = make_comment(&mut arena, "test comment");
        let clone = arena.deep_clone(comment);

        assert_ne!(clone, comment);
        if let NodeData::Comment(ref s) = arena.nodes[clone].data {
            assert_eq!(s, "test comment");
        } else {
            panic!("expected comment node");
        }
    }

    #[test]
    fn deep_clone_text_preserves_content() {
        let mut arena = Arena::new();
        let text = make_text(&mut arena, "hello world");
        let clone = arena.deep_clone(text);

        assert_ne!(clone, text);
        if let NodeData::Text(ref s) = arena.nodes[clone].data {
            assert_eq!(s, "hello world");
        } else {
            panic!("expected text node");
        }
    }

    #[test]
    fn deep_clone_is_detached() {
        let mut arena = Arena::new();
        let parent = make_element(&mut arena, "div");
        let child = make_element(&mut arena, "span");
        arena.append_child(parent, child);

        let clone = arena.deep_clone(child);
        assert_eq!(arena.nodes[clone].parent, None);
        assert_eq!(arena.nodes[clone].next_sibling, None);
        assert_eq!(arena.nodes[clone].prev_sibling, None);
    }

    // ─── shallow clone (importNode shallow) ──────────────────

    #[test]
    fn shallow_clone_no_children() {
        let mut arena = Arena::new();
        let div = make_element(&mut arena, "div");
        let child = make_element(&mut arena, "span");
        arena.append_child(div, child);

        // Shallow clone: copy just the node data, no children
        let data = arena.nodes[div].data.clone();
        let clone = arena.new_node(data);

        assert_ne!(clone, div);
        assert_eq!(arena.nodes[clone].first_child, None);
        assert_eq!(arena.nodes[clone].last_child, None);
    }

    #[test]
    fn shallow_clone_preserves_tag() {
        let mut arena = Arena::new();
        let span = make_element(&mut arena, "span");
        let data = arena.nodes[span].data.clone();
        let clone = arena.new_node(data);

        if let NodeData::Element(ref d) = arena.nodes[clone].data {
            assert_eq!(&*d.name.local, "span");
        } else {
            panic!("expected element");
        }
    }

    #[test]
    fn shallow_clone_preserves_attrs() {
        let mut arena = Arena::new();
        let div = make_element_with_attrs(&mut arena, "div", &[("id", "x"), ("data-val", "42")]);
        let data = arena.nodes[div].data.clone();
        let clone = arena.new_node(data);

        if let NodeData::Element(ref d) = arena.nodes[clone].data {
            assert_eq!(d.attrs.len(), 2);
        } else {
            panic!("expected element");
        }
    }

    // ─── adoptNode (detach + connected flags) ────────────────

    #[test]
    fn detach_removes_from_parent() {
        let mut arena = Arena::new();
        let parent = make_element(&mut arena, "div");
        let child = make_element(&mut arena, "span");
        arena.append_child(arena.document, parent);
        arena.append_child(parent, child);

        arena.detach(child);

        assert_eq!(arena.nodes[child].parent, None);
        assert_eq!(child_vec(&arena, parent), vec![]);
    }

    #[test]
    fn detach_clears_sibling_links() {
        let mut arena = Arena::new();
        let parent = make_element(&mut arena, "div");
        let a = make_element(&mut arena, "a");
        let b = make_element(&mut arena, "b");
        let c = make_element(&mut arena, "c");
        arena.append_child(parent, a);
        arena.append_child(parent, b);
        arena.append_child(parent, c);

        arena.detach(b);

        assert_eq!(arena.nodes[b].parent, None);
        assert_eq!(arena.nodes[b].next_sibling, None);
        assert_eq!(arena.nodes[b].prev_sibling, None);
        // a and c should now be siblings
        assert_eq!(arena.nodes[a].next_sibling, Some(c));
        assert_eq!(arena.nodes[c].prev_sibling, Some(a));
    }

    #[test]
    fn set_connected_recursive_clears_flags() {
        let mut arena = Arena::new();
        let parent = make_element(&mut arena, "div");
        let child = make_element(&mut arena, "span");
        let grandchild = make_text(&mut arena, "text");
        arena.append_child(arena.document, parent);
        arena.append_child(parent, child);
        arena.append_child(child, grandchild);

        // Mark connected
        arena.set_connected_recursive(parent, true);
        assert!(arena.nodes[parent].flags.is_connected());
        assert!(arena.nodes[child].flags.is_connected());
        assert!(arena.nodes[grandchild].flags.is_connected());

        // adoptNode semantics: detach + disconnect
        arena.detach(child);
        arena.set_connected_recursive(child, false);
        assert!(!arena.nodes[child].flags.is_connected());
        assert!(!arena.nodes[grandchild].flags.is_connected());
    }

    #[test]
    fn detach_orphan_is_noop() {
        let mut arena = Arena::new();
        let node = make_element(&mut arena, "div");
        // Detaching an already-orphaned node should not panic
        arena.detach(node);
        assert_eq!(arena.nodes[node].parent, None);
    }

    #[test]
    fn deep_clone_document_fragment() {
        let mut arena = Arena::new();
        let frag = arena.new_node(NodeData::DocumentFragment);
        let a = make_element(&mut arena, "div");
        let b = make_element(&mut arena, "span");
        arena.append_child(frag, a);
        arena.append_child(frag, b);

        let clone = arena.deep_clone(frag);
        assert_ne!(clone, frag);
        assert!(matches!(arena.nodes[clone].data, NodeData::DocumentFragment));
        assert_eq!(child_vec(&arena, clone).len(), 2);
    }

    // ─── Layout geometry helpers ─────────────────────────────────────

    #[test]
    fn absolute_position_sums_ancestors() {
        let mut arena = Arena::new();
        let html = make_element(&mut arena, "html");
        let body = make_element(&mut arena, "body");
        let div = make_element(&mut arena, "div");
        arena.append_child(arena.document, html);
        arena.append_child(html, body);
        arena.append_child(body, div);

        // Set layout positions
        arena.nodes[html].taffy_layout.location = taffy::Point { x: 0.0, y: 0.0 };
        arena.nodes[body].taffy_layout.location = taffy::Point { x: 8.0, y: 8.0 };
        arena.nodes[div].taffy_layout.location = taffy::Point { x: 10.0, y: 20.0 };

        let (x, y) = arena.absolute_position(div);
        assert_eq!(x, 18.0); // 0 + 8 + 10
        assert_eq!(y, 28.0); // 0 + 8 + 20
    }

    #[test]
    fn bounding_rect_combines_position_and_size() {
        let mut arena = Arena::new();
        let div = make_element(&mut arena, "div");
        arena.append_child(arena.document, div);

        arena.nodes[div].taffy_layout.location = taffy::Point { x: 50.0, y: 100.0 };
        arena.nodes[div].taffy_layout.size = taffy::Size { width: 200.0, height: 150.0 };

        let (x, y, w, h) = arena.bounding_rect(div);
        assert_eq!(x, 50.0);
        assert_eq!(y, 100.0);
        assert_eq!(w, 200.0);
        assert_eq!(h, 150.0);
    }

    #[test]
    fn content_box_subtracts_padding_and_border() {
        let mut arena = Arena::new();
        let div = make_element(&mut arena, "div");
        arena.append_child(arena.document, div);

        arena.nodes[div].taffy_layout.size = taffy::Size { width: 200.0, height: 100.0 };
        arena.nodes[div].taffy_layout.padding = taffy::Rect {
            left: 10.0, right: 10.0, top: 5.0, bottom: 5.0,
        };
        arena.nodes[div].taffy_layout.border = taffy::Rect {
            left: 2.0, right: 2.0, top: 1.0, bottom: 1.0,
        };

        let (cx, cy, cw, ch) = arena.content_box(div);
        assert_eq!(cx, 12.0);  // padding.left + border.left
        assert_eq!(cy, 6.0);   // padding.top + border.top
        assert_eq!(cw, 176.0); // 200 - 12 - 12
        assert_eq!(ch, 88.0);  // 100 - 6 - 6
    }

    #[test]
    fn content_box_clamps_to_zero() {
        let mut arena = Arena::new();
        let div = make_element(&mut arena, "div");
        arena.append_child(arena.document, div);

        // Element smaller than its padding+border
        arena.nodes[div].taffy_layout.size = taffy::Size { width: 10.0, height: 10.0 };
        arena.nodes[div].taffy_layout.padding = taffy::Rect {
            left: 20.0, right: 20.0, top: 20.0, bottom: 20.0,
        };

        let (_cx, _cy, cw, ch) = arena.content_box(div);
        assert_eq!(cw, 0.0);
        assert_eq!(ch, 0.0);
    }

    #[test]
    fn io_intersection_computation() {
        // Test the intersection math used by IntersectionObserver
        let mut arena = Arena::new();
        let html = make_element(&mut arena, "html");
        arena.append_child(arena.document, html);

        // Element fully in viewport
        let div_in = make_element(&mut arena, "div");
        arena.append_child(html, div_in);
        arena.nodes[div_in].taffy_layout.location = taffy::Point { x: 100.0, y: 100.0 };
        arena.nodes[div_in].taffy_layout.size = taffy::Size { width: 200.0, height: 150.0 };

        let (bx, by, bw, bh) = arena.bounding_rect(div_in);
        // Clip against viewport 0,0,1920,1080
        let ix = bx.max(0.0);
        let iy = by.max(0.0);
        let ix2 = (bx + bw).min(1920.0);
        let iy2 = (by + bh).min(1080.0);
        let iw = (ix2 - ix).max(0.0);
        let ih = (iy2 - iy).max(0.0);
        // Fully visible
        assert_eq!(iw, 200.0);
        assert_eq!(ih, 150.0);
        let ratio = (iw * ih) / (bw * bh);
        assert_eq!(ratio, 1.0);

        // Element partially outside viewport (extends below)
        let div_partial = make_element(&mut arena, "div");
        arena.append_child(html, div_partial);
        arena.nodes[div_partial].taffy_layout.location = taffy::Point { x: 0.0, y: 1000.0 };
        arena.nodes[div_partial].taffy_layout.size = taffy::Size { width: 100.0, height: 200.0 };

        let (_bx, by, bw, bh) = arena.bounding_rect(div_partial);
        let iy2 = (by + bh).min(1080.0);
        let ih = (iy2 - by.max(0.0)).max(0.0);
        // Only 80px visible (1080 - 1000)
        assert_eq!(ih, 80.0);
        let ratio = (bw * ih) / (bw * bh);
        assert!((ratio - 0.4).abs() < 0.01, "ratio should be ~0.4, got {}", ratio);
    }

    #[test]
    fn ro_content_box_through_full_pipeline() {
        // Full pipeline: parse → style → layout → read content box
        let mut arena = crate::dom::parse_document(
            "<html><head><style>#box { width: 200px; height: 100px; padding: 10px; border: 2px solid black; box-sizing: content-box; }</style></head><body><div id=\"box\"></div></body></html>",
        );
        crate::css::resolve::resolve_styles(&mut arena);
        crate::css::layout::compute_layout(&mut arena);

        let div = arena.find_element(arena.document, "div").unwrap();
        let layout = &arena.nodes[div].taffy_layout;

        // content-box: total = 200 + 10*2 + 2*2 = 224 wide, 100 + 10*2 + 2*2 = 124 tall
        assert_eq!(layout.size.width, 224.0);
        assert_eq!(layout.size.height, 124.0);

        let (_cx, _cy, cw, ch) = arena.content_box(div);
        assert_eq!(cw, 200.0);
        assert_eq!(ch, 100.0);
    }
