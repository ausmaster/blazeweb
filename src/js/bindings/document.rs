/// Document prototype bindings.
///
/// Installs accessors and methods specific to the Document interface.

use crate::dom::node::{ElementData, NodeData};
use crate::js::templates::{arena_mut, arena_ref, unwrap_node_id, wrap_node};

pub fn install(scope: &mut v8::HandleScope<()>, proto: &v8::Local<v8::ObjectTemplate>) {
    // Accessors
    set_accessor(scope, proto, "documentElement", document_element_getter);
    set_accessor(scope, proto, "head", head_getter);
    set_accessor_with_setter(scope, proto, "body", body_getter, body_setter);
    set_accessor_with_setter(scope, proto, "title", title_getter, title_setter);

    // Methods
    set_method(scope, proto, "getElementById", get_element_by_id);
    set_method(scope, proto, "getElementsByTagName", get_elements_by_tag_name);
    set_method(scope, proto, "getElementsByClassName", get_elements_by_class_name);
    set_method(scope, proto, "createElement", create_element);
    set_method(scope, proto, "createTextNode", create_text_node);
    set_method(scope, proto, "createComment", create_comment);
    set_method(scope, proto, "createDocumentFragment", create_document_fragment);
    set_method(scope, proto, "querySelector", query_selector);
    set_method(scope, proto, "querySelectorAll", query_selector_all);
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn set_accessor(
    scope: &mut v8::HandleScope<()>,
    proto: &v8::Local<v8::ObjectTemplate>,
    name: &str,
    getter: impl v8::MapFnTo<v8::FunctionCallback>,
) {
    let key = v8::String::new(scope, name).unwrap();
    let getter_ft = v8::FunctionTemplate::new(scope, getter);
    proto.set_accessor_property(key.into(), Some(getter_ft), None, v8::PropertyAttribute::NONE);
}

fn set_accessor_with_setter(
    scope: &mut v8::HandleScope<()>,
    proto: &v8::Local<v8::ObjectTemplate>,
    name: &str,
    getter: impl v8::MapFnTo<v8::FunctionCallback>,
    setter: impl v8::MapFnTo<v8::FunctionCallback>,
) {
    let key = v8::String::new(scope, name).unwrap();
    let getter_ft = v8::FunctionTemplate::new(scope, getter);
    let setter_ft = v8::FunctionTemplate::new(scope, setter);
    proto.set_accessor_property(key.into(), Some(getter_ft), Some(setter_ft), v8::PropertyAttribute::NONE);
}

fn set_method(
    scope: &mut v8::HandleScope<()>,
    proto: &v8::Local<v8::ObjectTemplate>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
) {
    let key = v8::String::new(scope, name).unwrap();
    let ft = v8::FunctionTemplate::new(scope, callback);
    proto.set(key.into(), ft.into());
}

// ─── Tree helpers ─────────────────────────────────────────────────────────────

/// Find the <html> element (first Element child of Document).
fn find_document_element(arena: &crate::dom::Arena) -> Option<crate::dom::NodeId> {
    for child in arena.children(arena.document) {
        if let NodeData::Element(data) = &arena.nodes[child].data {
            if &*data.name.local == "html" {
                return Some(child);
            }
        }
    }
    None
}

/// Find a direct child element by tag name within a parent.
fn find_child_element(
    arena: &crate::dom::Arena,
    parent: crate::dom::NodeId,
    tag: &str,
) -> Option<crate::dom::NodeId> {
    for child in arena.children(parent) {
        if let NodeData::Element(data) = &arena.nodes[child].data {
            if &*data.name.local == tag {
                return Some(child);
            }
        }
    }
    None
}

// ─── Accessors ────────────────────────────────────────────────────────────────

fn document_element_getter(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let arena = arena_ref(scope);
    match find_document_element(arena) {
        Some(id) => rv.set(wrap_node(scope, id).into()),
        None => rv.set(v8::null(scope).into()),
    }
}

fn head_getter(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let arena = arena_ref(scope);
    if let Some(html) = find_document_element(arena) {
        if let Some(head) = find_child_element(arena, html, "head") {
            rv.set(wrap_node(scope, head).into());
            return;
        }
    }
    rv.set(v8::null(scope).into());
}

fn body_getter(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let arena = arena_ref(scope);
    if let Some(html) = find_document_element(arena) {
        if let Some(body) = find_child_element(arena, html, "body") {
            rv.set(wrap_node(scope, body).into());
            return;
        }
    }
    rv.set(v8::null(scope).into());
}

fn body_setter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let value = args.get(0);
    if !value.is_object() {
        return;
    }
    let obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(value) };
    let Some(new_body_id) = unwrap_node_id(scope, obj) else { return };

    let arena = arena_mut(scope);
    let Some(html) = find_document_element(arena) else { return };

    // Remove old body if present
    if let Some(old_body) = find_child_element(arena, html, "body") {
        arena.detach(old_body);
    }
    if arena.nodes[new_body_id].parent.is_some() {
        arena.detach(new_body_id);
    }
    arena.append_child(html, new_body_id);
}

fn title_getter(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let arena = arena_ref(scope);
    if let Some(html) = find_document_element(arena) {
        if let Some(head) = find_child_element(arena, html, "head") {
            if let Some(title_el) = find_child_element(arena, head, "title") {
                // Collect text content of <title>
                let mut text = String::new();
                for child in arena.children(title_el) {
                    if let NodeData::Text(s) = &arena.nodes[child].data {
                        text.push_str(s);
                    }
                }
                let v8_str = v8::String::new(scope, text.trim()).unwrap();
                rv.set(v8_str.into());
                return;
            }
        }
    }
    let v8_str = v8::String::new(scope, "").unwrap();
    rv.set(v8_str.into());
}

fn title_setter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let text = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_mut(scope);
    let Some(html) = find_document_element(arena) else { return };
    let Some(head) = find_child_element(arena, html, "head") else { return };

    if let Some(title_el) = find_child_element(arena, head, "title") {
        arena.remove_all_children(title_el);
        let text_node = arena.new_node(NodeData::Text(text));
        arena.append_child(title_el, text_node);
    } else {
        // Create <title> element
        let name = markup5ever::QualName::new(None, markup5ever::ns!(html), "title".into());
        let title_el = arena.new_node(NodeData::Element(ElementData::new(name, vec![])));
        let text_node = arena.new_node(NodeData::Text(text));
        arena.append_child(title_el, text_node);
        arena.append_child(head, title_el);
    }
}

// ─── Methods ──────────────────────────────────────────────────────────────────

fn get_element_by_id(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let id_str = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_ref(scope);

    // Linear scan all nodes for matching id attribute
    for (node_id, node) in &arena.nodes {
        if let NodeData::Element(data) = &node.data {
            if data.get_attribute("id") == Some(&id_str) {
                let wrapped = wrap_node(scope, node_id);
                rv.set(wrapped.into());
                return;
            }
        }
    }
    rv.set(v8::null(scope).into());
}

fn get_elements_by_tag_name(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let tag = args.get(0).to_rust_string_lossy(scope).to_ascii_lowercase();
    let arena = arena_ref(scope);
    let mut results = Vec::new();

    collect_elements_by_tag(arena, arena.document, &tag, &mut results);

    let arr = v8::Array::new(scope, results.len() as i32);
    for (i, id) in results.iter().enumerate() {
        let wrapped = wrap_node(scope, *id);
        arr.set_index(scope, i as u32, wrapped.into());
    }
    rv.set(arr.into());
}

fn collect_elements_by_tag(
    arena: &crate::dom::Arena,
    node: crate::dom::NodeId,
    tag: &str,
    results: &mut Vec<crate::dom::NodeId>,
) {
    for child in arena.children(node) {
        if let NodeData::Element(data) = &arena.nodes[child].data {
            if tag == "*" || &*data.name.local == tag {
                results.push(child);
            }
        }
        collect_elements_by_tag(arena, child, tag, results);
    }
}

fn get_elements_by_class_name(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let class_names = args.get(0).to_rust_string_lossy(scope);
    let wanted: Vec<&str> = class_names.split_whitespace().collect();
    if wanted.is_empty() {
        let arr = v8::Array::new(scope, 0);
        rv.set(arr.into());
        return;
    }

    let arena = arena_ref(scope);
    let mut results = Vec::new();

    collect_elements_by_class(arena, arena.document, &wanted, &mut results);

    let arr = v8::Array::new(scope, results.len() as i32);
    for (i, id) in results.iter().enumerate() {
        let wrapped = wrap_node(scope, *id);
        arr.set_index(scope, i as u32, wrapped.into());
    }
    rv.set(arr.into());
}

fn collect_elements_by_class(
    arena: &crate::dom::Arena,
    node: crate::dom::NodeId,
    wanted: &[&str],
    results: &mut Vec<crate::dom::NodeId>,
) {
    for child in arena.children(node) {
        if let NodeData::Element(data) = &arena.nodes[child].data {
            if let Some(class_attr) = data.get_attribute("class") {
                let classes: Vec<&str> = class_attr.split_whitespace().collect();
                if wanted.iter().all(|w| classes.contains(w)) {
                    results.push(child);
                }
            }
        }
        collect_elements_by_class(arena, child, wanted, results);
    }
}

fn create_element(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let tag = args.get(0).to_rust_string_lossy(scope).to_ascii_lowercase();
    let arena = arena_mut(scope);
    let name = markup5ever::QualName::new(None, markup5ever::ns!(html), tag.into());
    let node_id = arena.new_node(NodeData::Element(ElementData::new(name, vec![])));
    let wrapped = wrap_node(scope, node_id);
    rv.set(wrapped.into());
}

fn create_text_node(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let text = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_mut(scope);
    let node_id = arena.new_node(NodeData::Text(text));
    let wrapped = wrap_node(scope, node_id);
    rv.set(wrapped.into());
}

fn create_comment(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let text = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_mut(scope);
    let node_id = arena.new_node(NodeData::Comment(text));
    let wrapped = wrap_node(scope, node_id);
    rv.set(wrapped.into());
}

fn create_document_fragment(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let arena = arena_mut(scope);
    let node_id = arena.new_node(NodeData::Document);
    let wrapped = wrap_node(scope, node_id);
    rv.set(wrapped.into());
}

fn query_selector(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    // Stub: full selectors crate integration deferred
    rv.set(v8::null(scope).into());
}

fn query_selector_all(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    // Stub: full selectors crate integration deferred
    let arr = v8::Array::new(scope, 0);
    rv.set(arr.into());
}
