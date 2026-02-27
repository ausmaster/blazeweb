/// Node prototype bindings.
///
/// Installs accessors and methods on the Node prototype template, which
/// Document, Element, Text, and Comment all inherit from.

use crate::dom::node::NodeData;
use crate::js::templates::{arena_mut, arena_ref, unwrap_node_id, wrap_node};

/// Install Node properties and methods on the given prototype template.
pub fn install(scope: &mut v8::HandleScope<()>, proto: &v8::Local<v8::ObjectTemplate>) {
    // Readonly accessors
    set_accessor(scope, proto, "nodeType", node_type_getter);
    set_accessor(scope, proto, "nodeName", node_name_getter);
    set_accessor(scope, proto, "nodeValue", node_value_getter);
    set_accessor(scope, proto, "parentNode", parent_node_getter);
    set_accessor(scope, proto, "parentElement", parent_element_getter);
    set_accessor(scope, proto, "firstChild", first_child_getter);
    set_accessor(scope, proto, "lastChild", last_child_getter);
    set_accessor(scope, proto, "nextSibling", next_sibling_getter);
    set_accessor(scope, proto, "previousSibling", previous_sibling_getter);
    set_accessor(scope, proto, "childNodes", child_nodes_getter);
    set_accessor(scope, proto, "ownerDocument", owner_document_getter);

    // Read-write accessor
    set_accessor_with_setter(scope, proto, "textContent", text_content_getter, text_content_setter);

    // Methods
    set_method(scope, proto, "appendChild", append_child);
    set_method(scope, proto, "removeChild", remove_child);
    set_method(scope, proto, "insertBefore", insert_before);
    set_method(scope, proto, "cloneNode", clone_node);
    set_method(scope, proto, "hasChildNodes", has_child_nodes);
    set_method(scope, proto, "contains", contains);
    set_method(scope, proto, "isSameNode", is_same_node);
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

// ─── Accessors ────────────────────────────────────────────────────────────────

fn node_type_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let node_type = match &arena.nodes[node_id].data {
        NodeData::Element(_) => 1,
        NodeData::Text(_) => 3,
        NodeData::Comment(_) => 8,
        NodeData::Document => 9,
        NodeData::Doctype { .. } => 10,
    };
    rv.set(v8::Integer::new(scope, node_type).into());
}

fn node_name_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let name: String = match &arena.nodes[node_id].data {
        NodeData::Element(data) => data.name.local.to_ascii_uppercase().to_string(),
        NodeData::Text(_) => "#text".to_string(),
        NodeData::Comment(_) => "#comment".to_string(),
        NodeData::Document => "#document".to_string(),
        NodeData::Doctype { .. } => "#doctype".to_string(),
    };
    let v8_str = v8::String::new(scope, &name).unwrap();
    rv.set(v8_str.into());
}

fn node_value_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    match &arena.nodes[node_id].data {
        NodeData::Text(s) | NodeData::Comment(s) => {
            let v8_str = v8::String::new(scope, s).unwrap();
            rv.set(v8_str.into());
        }
        _ => rv.set(v8::null(scope).into()),
    }
}

fn parent_node_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    match arena.nodes[node_id].parent {
        Some(parent_id) => {
            let wrapped = wrap_node(scope, parent_id);
            rv.set(wrapped.into());
        }
        None => rv.set(v8::null(scope).into()),
    }
}

fn parent_element_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    match arena.nodes[node_id].parent {
        Some(parent_id) if matches!(&arena.nodes[parent_id].data, NodeData::Element(_)) => {
            let wrapped = wrap_node(scope, parent_id);
            rv.set(wrapped.into());
        }
        _ => rv.set(v8::null(scope).into()),
    }
}

fn first_child_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    match arena.nodes[node_id].first_child {
        Some(child_id) => rv.set(wrap_node(scope, child_id).into()),
        None => rv.set(v8::null(scope).into()),
    }
}

fn last_child_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    match arena.nodes[node_id].last_child {
        Some(child_id) => rv.set(wrap_node(scope, child_id).into()),
        None => rv.set(v8::null(scope).into()),
    }
}

fn next_sibling_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    match arena.nodes[node_id].next_sibling {
        Some(sib_id) => rv.set(wrap_node(scope, sib_id).into()),
        None => rv.set(v8::null(scope).into()),
    }
}

fn previous_sibling_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    match arena.nodes[node_id].prev_sibling {
        Some(sib_id) => rv.set(wrap_node(scope, sib_id).into()),
        None => rv.set(v8::null(scope).into()),
    }
}

fn child_nodes_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let children: Vec<_> = arena.children(node_id).collect();
    let arr = v8::Array::new(scope, children.len() as i32);
    for (i, child_id) in children.iter().enumerate() {
        let wrapped = wrap_node(scope, *child_id);
        arr.set_index(scope, i as u32, wrapped.into());
    }
    rv.set(arr.into());
}

fn owner_document_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    // The document node's ownerDocument is null per spec
    if matches!(&arena.nodes[node_id].data, NodeData::Document) {
        rv.set(v8::null(scope).into());
    } else {
        let doc = wrap_node(scope, arena.document);
        rv.set(doc.into());
    }
}

fn text_content_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let text = collect_text_content(arena, node_id);
    let v8_str = v8::String::new(scope, &text).unwrap();
    rv.set(v8_str.into());
}

fn text_content_setter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let value = args.get(0);
    let text = value.to_rust_string_lossy(scope);
    let arena = arena_mut(scope);
    arena.remove_all_children(node_id);
    if !text.is_empty() {
        let text_node = arena.new_node(NodeData::Text(text));
        arena.append_child(node_id, text_node);
    }
}

/// Recursively collect text content of a node.
fn collect_text_content(arena: &crate::dom::Arena, node_id: crate::dom::NodeId) -> String {
    let mut result = String::new();
    collect_text_recursive(arena, node_id, &mut result);
    result
}

fn collect_text_recursive(
    arena: &crate::dom::Arena,
    node_id: crate::dom::NodeId,
    buf: &mut String,
) {
    match &arena.nodes[node_id].data {
        NodeData::Text(s) => buf.push_str(s),
        _ => {
            for child in arena.children(node_id) {
                collect_text_recursive(arena, child, buf);
            }
        }
    }
}

// ─── Methods ──────────────────────────────────────────────────────────────────

fn append_child(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(parent_id) = unwrap_node_id(scope, args.this()) else { return };
    let child_arg = args.get(0);
    if !child_arg.is_object() {
        let msg = v8::String::new(scope, "appendChild: argument is not a Node").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    }
    let child_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(child_arg) };
    let Some(child_id) = unwrap_node_id(scope, child_obj) else {
        let msg = v8::String::new(scope, "appendChild: argument is not a Node").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    };

    let arena = arena_mut(scope);
    // Detach from current parent if any (spec: re-parenting)
    if arena.nodes[child_id].parent.is_some() {
        arena.detach(child_id);
    }
    arena.append_child(parent_id, child_id);
    rv.set(child_arg);
}

fn remove_child(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(_parent_id) = unwrap_node_id(scope, args.this()) else { return };
    let child_arg = args.get(0);
    if !child_arg.is_object() {
        let msg = v8::String::new(scope, "removeChild: argument is not a Node").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    }
    let child_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(child_arg) };
    let Some(child_id) = unwrap_node_id(scope, child_obj) else {
        let msg = v8::String::new(scope, "removeChild: argument is not a Node").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    };

    let arena = arena_mut(scope);
    arena.detach(child_id);
    rv.set(child_arg);
}

fn insert_before(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(parent_id) = unwrap_node_id(scope, args.this()) else { return };
    let new_node_arg = args.get(0);
    let ref_node_arg = args.get(1);

    if !new_node_arg.is_object() {
        let msg = v8::String::new(scope, "insertBefore: first argument is not a Node").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    }
    let new_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(new_node_arg) };
    let Some(new_id) = unwrap_node_id(scope, new_obj) else {
        let msg = v8::String::new(scope, "insertBefore: first argument is not a Node").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    };

    let arena = arena_mut(scope);
    // Detach from current parent if any
    if arena.nodes[new_id].parent.is_some() {
        arena.detach(new_id);
    }

    if ref_node_arg.is_null() || ref_node_arg.is_undefined() {
        // insertBefore(node, null) acts like appendChild
        arena.append_child(parent_id, new_id);
    } else if ref_node_arg.is_object() {
        let ref_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(ref_node_arg) };
        if let Some(ref_id) = unwrap_node_id(scope, ref_obj) {
            arena.insert_before(ref_id, new_id);
        }
    }

    rv.set(new_node_arg);
}

fn clone_node(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let deep = args.get(0).boolean_value(scope);

    let arena = arena_mut(scope);
    let clone_id = if deep {
        arena.deep_clone(node_id)
    } else {
        // Shallow clone: copy node data only, no children
        let data = arena.nodes[node_id].data.clone();
        arena.new_node(data)
    };

    let wrapped = wrap_node(scope, clone_id);
    rv.set(wrapped.into());
}

fn has_child_nodes(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let has = arena.nodes[node_id].first_child.is_some();
    rv.set(v8::Boolean::new(scope, has).into());
}

fn contains(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let other_arg = args.get(0);
    if !other_arg.is_object() {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    }
    let other_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(other_arg) };
    let Some(other_id) = unwrap_node_id(scope, other_obj) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };

    // Walk up from other_id to see if we hit node_id
    let arena = arena_ref(scope);
    let mut current = Some(other_id);
    while let Some(id) = current {
        if id == node_id {
            rv.set(v8::Boolean::new(scope, true).into());
            return;
        }
        current = arena.nodes[id].parent;
    }
    rv.set(v8::Boolean::new(scope, false).into());
}

fn is_same_node(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let other_arg = args.get(0);
    if !other_arg.is_object() {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    }
    let other_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(other_arg) };
    let Some(other_id) = unwrap_node_id(scope, other_obj) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    rv.set(v8::Boolean::new(scope, node_id == other_id).into());
}
