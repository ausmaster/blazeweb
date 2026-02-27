/// Element prototype bindings.
///
/// Installs accessors and methods specific to the Element interface
/// (tagName, id, className, getAttribute/setAttribute, innerHTML, etc.)

use crate::dom::node::NodeData;
use crate::js::templates::{arena_mut, arena_ref, unwrap_node_id, wrap_node};

pub fn install(scope: &mut v8::HandleScope<()>, proto: &v8::Local<v8::ObjectTemplate>) {
    // Readonly accessors
    set_accessor(scope, proto, "tagName", tag_name_getter);
    set_accessor(scope, proto, "outerHTML", outer_html_getter);
    set_accessor(scope, proto, "children", children_getter);
    set_accessor(scope, proto, "childElementCount", child_element_count_getter);
    set_accessor(scope, proto, "firstElementChild", first_element_child_getter);
    set_accessor(scope, proto, "lastElementChild", last_element_child_getter);
    set_accessor(scope, proto, "nextElementSibling", next_element_sibling_getter);
    set_accessor(scope, proto, "previousElementSibling", previous_element_sibling_getter);

    // Read-write accessors
    set_accessor_with_setter(scope, proto, "id", id_getter, id_setter);
    set_accessor_with_setter(scope, proto, "className", class_name_getter, class_name_setter);
    set_accessor_with_setter(scope, proto, "innerHTML", inner_html_getter, inner_html_setter);

    // Methods
    set_method(scope, proto, "getAttribute", get_attribute);
    set_method(scope, proto, "setAttribute", set_attribute);
    set_method(scope, proto, "removeAttribute", remove_attribute);
    set_method(scope, proto, "hasAttribute", has_attribute);
    set_method(scope, proto, "remove", remove);
    set_method(scope, proto, "matches", matches_stub);
    set_method(scope, proto, "closest", closest_stub);
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

fn tag_name_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    if let NodeData::Element(data) = &arena.nodes[node_id].data {
        let tag = data.name.local.to_ascii_uppercase();
        let v8_str = v8::String::new(scope, &tag).unwrap();
        rv.set(v8_str.into());
    }
}

fn id_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    if let NodeData::Element(data) = &arena.nodes[node_id].data {
        let id = data.get_attribute("id").unwrap_or("");
        let v8_str = v8::String::new(scope, id).unwrap();
        rv.set(v8_str.into());
    }
}

fn id_setter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let val = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_mut(scope);
    if let NodeData::Element(data) = &mut arena.nodes[node_id].data {
        data.set_attribute("id", &val);
    }
}

fn class_name_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    if let NodeData::Element(data) = &arena.nodes[node_id].data {
        let cls = data.get_attribute("class").unwrap_or("");
        let v8_str = v8::String::new(scope, cls).unwrap();
        rv.set(v8_str.into());
    }
}

fn class_name_setter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let val = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_mut(scope);
    if let NodeData::Element(data) = &mut arena.nodes[node_id].data {
        data.set_attribute("class", &val);
    }
}

fn inner_html_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);

    // Serialize all children of this element
    let mut output = String::new();
    for child in arena.children(node_id) {
        serialize_node(arena, child, &mut output);
    }
    let v8_str = v8::String::new(scope, &output).unwrap();
    rv.set(v8_str.into());
}

fn inner_html_setter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let html = args.get(0).to_rust_string_lossy(scope);

    let arena = arena_mut(scope);

    // Get the context element tag for fragment parsing
    let tag = match &arena.nodes[node_id].data {
        NodeData::Element(data) => data.name.local.to_string(),
        _ => return,
    };

    // Remove existing children
    arena.remove_all_children(node_id);

    if html.is_empty() {
        return;
    }

    // Parse fragment into a temporary arena
    let fragment_arena = crate::dom::treesink::parse_fragment(&html, &tag, true);

    // Transfer nodes from fragment arena into main arena.
    // Fragment produces: Document → <html> wrapper → actual children
    if let Some(html_wrapper) = fragment_arena.children(fragment_arena.document).next() {
        for child in fragment_arena.children(html_wrapper) {
            let new_id = clone_across_arenas(arena, &fragment_arena, child);
            arena.append_child(node_id, new_id);
        }
    }
}

fn outer_html_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let mut output = String::new();
    serialize_node(arena, node_id, &mut output);
    let v8_str = v8::String::new(scope, &output).unwrap();
    rv.set(v8_str.into());
}

fn children_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let element_children: Vec<_> = arena
        .children(node_id)
        .filter(|&id| matches!(&arena.nodes[id].data, NodeData::Element(_)))
        .collect();
    let arr = v8::Array::new(scope, element_children.len() as i32);
    for (i, id) in element_children.iter().enumerate() {
        let wrapped = wrap_node(scope, *id);
        arr.set_index(scope, i as u32, wrapped.into());
    }
    rv.set(arr.into());
}

fn child_element_count_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let count = arena
        .children(node_id)
        .filter(|&id| matches!(&arena.nodes[id].data, NodeData::Element(_)))
        .count();
    rv.set(v8::Integer::new(scope, count as i32).into());
}

fn first_element_child_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    for child in arena.children(node_id) {
        if matches!(&arena.nodes[child].data, NodeData::Element(_)) {
            rv.set(wrap_node(scope, child).into());
            return;
        }
    }
    rv.set(v8::null(scope).into());
}

fn last_element_child_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let mut last = None;
    for child in arena.children(node_id) {
        if matches!(&arena.nodes[child].data, NodeData::Element(_)) {
            last = Some(child);
        }
    }
    match last {
        Some(id) => rv.set(wrap_node(scope, id).into()),
        None => rv.set(v8::null(scope).into()),
    }
}

fn next_element_sibling_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let mut current = arena.nodes[node_id].next_sibling;
    while let Some(id) = current {
        if matches!(&arena.nodes[id].data, NodeData::Element(_)) {
            rv.set(wrap_node(scope, id).into());
            return;
        }
        current = arena.nodes[id].next_sibling;
    }
    rv.set(v8::null(scope).into());
}

fn previous_element_sibling_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let mut current = arena.nodes[node_id].prev_sibling;
    while let Some(id) = current {
        if matches!(&arena.nodes[id].data, NodeData::Element(_)) {
            rv.set(wrap_node(scope, id).into());
            return;
        }
        current = arena.nodes[id].prev_sibling;
    }
    rv.set(v8::null(scope).into());
}

// ─── Methods ──────────────────────────────────────────────────────────────────

fn get_attribute(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let attr_name = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_ref(scope);
    if let NodeData::Element(data) = &arena.nodes[node_id].data {
        match data.get_attribute(&attr_name) {
            Some(val) => {
                let v8_str = v8::String::new(scope, val).unwrap();
                rv.set(v8_str.into());
            }
            None => rv.set(v8::null(scope).into()),
        }
    }
}

fn set_attribute(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let attr_name = args.get(0).to_rust_string_lossy(scope);
    let attr_value = args.get(1).to_rust_string_lossy(scope);
    let arena = arena_mut(scope);
    if let NodeData::Element(data) = &mut arena.nodes[node_id].data {
        data.set_attribute(&attr_name, &attr_value);
    }
}

fn remove_attribute(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let attr_name = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_mut(scope);
    if let NodeData::Element(data) = &mut arena.nodes[node_id].data {
        data.remove_attribute(&attr_name);
    }
}

fn has_attribute(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let attr_name = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_ref(scope);
    if let NodeData::Element(data) = &arena.nodes[node_id].data {
        let has = data.get_attribute(&attr_name).is_some();
        rv.set(v8::Boolean::new(scope, has).into());
    } else {
        rv.set(v8::Boolean::new(scope, false).into());
    }
}

fn remove(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_mut(scope);
    if arena.nodes[node_id].parent.is_some() {
        arena.detach(node_id);
    }
}

fn matches_stub(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    // Stub: full selectors integration deferred
    rv.set(v8::Boolean::new(scope, false).into());
}

fn closest_stub(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    // Stub: full selectors integration deferred
    rv.set(v8::null(scope).into());
}

// ─── Serialization helpers ────────────────────────────────────────────────────

/// Reuse the existing serializer logic for innerHTML/outerHTML.
fn serialize_node(arena: &crate::dom::Arena, id: crate::dom::NodeId, output: &mut String) {
    crate::dom::serialize::serialize_node_to_string(arena, id, output);
}

/// Clone a node tree from one Arena into another (for innerHTML setter).
fn clone_across_arenas(
    dst: &mut crate::dom::Arena,
    src: &crate::dom::Arena,
    src_id: crate::dom::NodeId,
) -> crate::dom::NodeId {
    let data = src.nodes[src_id].data.clone();
    let new_id = dst.new_node(data);
    for child in src.children(src_id) {
        let child_id = clone_across_arenas(dst, src, child);
        dst.append_child(new_id, child_id);
    }
    new_id
}
