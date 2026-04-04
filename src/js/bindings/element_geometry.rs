/// Element geometry, insertAdjacent*, and miscellaneous DOM methods.

use crate::dom::node::NodeData;
use crate::js::templates::{arena_mut, arena_ref, unwrap_node_id, wrap_node};

pub(super) fn insert_adjacent_html(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let position = args.get(0).to_rust_string_lossy(scope).to_ascii_lowercase();
    let html = args.get(1).to_rust_string_lossy(scope);

    let arena = arena_mut(scope);
    let tag = match &arena.nodes[node_id].data {
        NodeData::Element(data) => data.name.local.to_string(),
        _ => return,
    };

    let fragment_arena = crate::dom::treesink::parse_fragment(&html, &tag, true);
    let mut new_nodes = Vec::new();
    if let Some(html_wrapper) = fragment_arena.children(fragment_arena.document).next() {
        for child in fragment_arena.children(html_wrapper) {
            let new_id = super::element::clone_across_arenas(arena, &fragment_arena, child);
            new_nodes.push(new_id);
        }
    }

    match position.as_str() {
        "beforebegin" => {
            // Insert before this element
            for new_id in new_nodes {
                arena.insert_before(node_id, new_id);
            }
        }
        "afterbegin" => {
            // Insert as first children
            let first_child = arena.nodes[node_id].first_child;
            if let Some(fc) = first_child {
                for new_id in new_nodes {
                    arena.insert_before(fc, new_id);
                }
            } else {
                for new_id in new_nodes {
                    arena.append_child(node_id, new_id);
                }
            }
        }
        "beforeend" => {
            // Append as last children
            for new_id in new_nodes {
                arena.append_child(node_id, new_id);
            }
        }
        "afterend" => {
            // Insert after this element
            let next = arena.nodes[node_id].next_sibling;
            if let Some(next_id) = next {
                for new_id in new_nodes {
                    arena.insert_before(next_id, new_id);
                }
            } else if let Some(parent_id) = arena.nodes[node_id].parent {
                for new_id in new_nodes {
                    arena.append_child(parent_id, new_id);
                }
            }
        }
        _ => {}
    }
}

pub(super) fn insert_adjacent_element(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let position = args.get(0).to_rust_string_lossy(scope).to_ascii_lowercase();
    let elem_arg = args.get(1);
    if !elem_arg.is_object() {
        rv.set(v8::null(scope).into());
        return;
    }
    let elem_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(elem_arg) };
    let Some(elem_id) = unwrap_node_id(scope, elem_obj) else {
        rv.set(v8::null(scope).into());
        return;
    };

    let arena = arena_mut(scope);
    if arena.nodes[elem_id].parent.is_some() {
        arena.detach(elem_id);
    }

    match position.as_str() {
        "beforebegin" => arena.insert_before(node_id, elem_id),
        "afterbegin" => {
            if let Some(fc) = arena.nodes[node_id].first_child {
                arena.insert_before(fc, elem_id);
            } else {
                arena.append_child(node_id, elem_id);
            }
        }
        "beforeend" => arena.append_child(node_id, elem_id),
        "afterend" => {
            if let Some(next) = arena.nodes[node_id].next_sibling {
                arena.insert_before(next, elem_id);
            } else if let Some(parent) = arena.nodes[node_id].parent {
                arena.append_child(parent, elem_id);
            }
        }
        _ => {
            rv.set(v8::null(scope).into());
            return;
        }
    }
    rv.set(elem_arg);
}

pub(super) fn get_bounding_client_rect(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let layout = &arena.nodes[node_id].taffy_layout;
    let (abs_x, abs_y) = arena.absolute_position(node_id);
    let width = layout.size.width as f64;
    let height = layout.size.height as f64;
    let x = abs_x as f64;
    let y = abs_y as f64;
    log::trace!(
        "getBoundingClientRect({:?}): x={:.1} y={:.1} w={:.1} h={:.1}",
        node_id, x, y, width, height
    );

    let obj = v8::Object::new(scope);
    let set = |s: &mut v8::HandleScope, o: v8::Local<v8::Object>, k: &str, v: f64| {
        let key = v8::String::new(s, k).unwrap();
        let val = v8::Number::new(s, v);
        o.set(s, key.into(), val.into());
    };
    set(scope, obj, "x", x);
    set(scope, obj, "y", y);
    set(scope, obj, "left", x);
    set(scope, obj, "top", y);
    set(scope, obj, "right", x + width);
    set(scope, obj, "bottom", y + height);
    set(scope, obj, "width", width);
    set(scope, obj, "height", height);
    rv.set(obj.into());
}

pub(super) fn offset_width_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let width = arena.nodes[node_id].taffy_layout.size.width;
    rv.set(v8::Number::new(scope, width as f64).into());
}

pub(super) fn offset_height_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let height = arena.nodes[node_id].taffy_layout.size.height;
    rv.set(v8::Number::new(scope, height as f64).into());
}

pub(super) fn client_width_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let layout = &arena.nodes[node_id].taffy_layout;
    let width = layout.size.width - layout.border.left - layout.border.right;
    rv.set(v8::Number::new(scope, width.max(0.0) as f64).into());
}

pub(super) fn client_height_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let layout = &arena.nodes[node_id].taffy_layout;
    let height = layout.size.height - layout.border.top - layout.border.bottom;
    rv.set(v8::Number::new(scope, height.max(0.0) as f64).into());
}

pub(super) fn scroll_width_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let layout = &arena.nodes[node_id].taffy_layout;
    let client_w = layout.size.width - layout.border.left - layout.border.right;
    let scroll_w = client_w.max(layout.content_size.width);
    rv.set(v8::Number::new(scope, scroll_w.max(0.0) as f64).into());
}

pub(super) fn scroll_height_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let layout = &arena.nodes[node_id].taffy_layout;
    let client_h = layout.size.height - layout.border.top - layout.border.bottom;
    let scroll_h = client_h.max(layout.content_size.height);
    rv.set(v8::Number::new(scope, scroll_h.max(0.0) as f64).into());
}

pub(super) fn offset_top_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let top = arena.nodes[node_id].taffy_layout.location.y;
    rv.set(v8::Number::new(scope, top as f64).into());
}

pub(super) fn offset_left_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let left = arena.nodes[node_id].taffy_layout.location.x;
    rv.set(v8::Number::new(scope, left as f64).into());
}

/// Zero getter for properties that don't have layout values (scrollTop, scrollLeft).
pub(super) fn geometry_zero_getter(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    rv.set(v8::Integer::new(scope, 0).into());
}

pub(super) fn element_noop(
    _scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
}

pub(super) fn get_client_rects(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let layout = &arena.nodes[node_id].taffy_layout;
    let (abs_x, abs_y) = arena.absolute_position(node_id);
    let width = layout.size.width as f64;
    let height = layout.size.height as f64;
    let x = abs_x as f64;
    let y = abs_y as f64;

    let arr = v8::Array::new(scope, 1);
    let rect = v8::Object::new(scope);
    let set = |s: &mut v8::HandleScope, o: v8::Local<v8::Object>, k: &str, v: f64| {
        let key = v8::String::new(s, k).unwrap();
        let val = v8::Number::new(s, v);
        o.set(s, key.into(), val.into());
    };
    set(scope, rect, "x", x);
    set(scope, rect, "y", y);
    set(scope, rect, "left", x);
    set(scope, rect, "top", y);
    set(scope, rect, "right", x + width);
    set(scope, rect, "bottom", y + height);
    set(scope, rect, "width", width);
    set(scope, rect, "height", height);
    arr.set_index(scope, 0, rect.into());
    rv.set(arr.into());
}

pub(super) fn get_attribute_names(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    if let NodeData::Element(data) = &arena.nodes[node_id].data {
        let arr = v8::Array::new(scope, data.attrs.len() as i32);
        for (i, attr) in data.attrs.iter().enumerate() {
            let v = v8::String::new(scope, &attr.name.local).unwrap();
            arr.set_index(scope, i as u32, v.into());
        }
        rv.set(arr.into());
    } else {
        rv.set(v8::Array::new(scope, 0).into());
    }
}

pub(super) fn has_attributes(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let has = if let NodeData::Element(data) = &arena.nodes[node_id].data {
        !data.attrs.is_empty()
    } else {
        false
    };
    rv.set(v8::Boolean::new(scope, has).into());
}

pub(super) fn toggle_attribute(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let attr_name = args.get(0).to_rust_string_lossy(scope);
    let force = if args.length() > 1 && !args.get(1).is_undefined() {
        Some(args.get(1).boolean_value(scope))
    } else {
        None
    };
    let arena = arena_mut(scope);
    if let NodeData::Element(data) = &mut arena.nodes[node_id].data {
        let has = data.get_attribute(&attr_name).is_some();
        let result = match force {
            Some(true) => {
                if !has { data.set_attribute(&attr_name, ""); }
                true
            }
            Some(false) => {
                if has { data.remove_attribute(&attr_name); }
                false
            }
            None => {
                if has {
                    data.remove_attribute(&attr_name);
                    false
                } else {
                    data.set_attribute(&attr_name, "");
                    true
                }
            }
        };
        rv.set(v8::Boolean::new(scope, result).into());
    }
}

pub(super) fn get_attribute_node(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let attr_name = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_ref(scope);
    if let NodeData::Element(data) = &arena.nodes[node_id].data {
        if let Some(val) = data.get_attribute(&attr_name) {
            let obj = v8::Object::new(scope);
            let k = v8::String::new(scope, "name").unwrap();
            let v = v8::String::new(scope, &attr_name).unwrap();
            obj.set(scope, k.into(), v.into());
            let k = v8::String::new(scope, "value").unwrap();
            let v = v8::String::new(scope, val).unwrap();
            obj.set(scope, k.into(), v.into());
            let k = v8::String::new(scope, "specified").unwrap();
            let v = v8::Boolean::new(scope, true);
            obj.set(scope, k.into(), v.into());
            rv.set(obj.into());
            return;
        }
    }
    rv.set(v8::null(scope).into());
}

pub(super) fn attach_shadow(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    // Return a DocumentFragment as minimal shadow root
    let arena = arena_mut(scope);
    let frag = arena.new_node(NodeData::Document);
    rv.set(wrap_node(scope, frag).into());
}

pub(super) fn element_animate(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    // Return a minimal Animation-like object
    let obj = v8::Object::new(scope);
    let k = v8::String::new(scope, "finished").unwrap();
    // Promise.resolve() stub
    let resolver = v8::PromiseResolver::new(scope).unwrap();
    let undef = v8::undefined(scope);
    resolver.resolve(scope, undef.into());
    let promise = resolver.get_promise(scope);
    obj.set(scope, k.into(), promise.into());
    let noop = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    for name in &["play", "pause", "cancel", "finish", "reverse"] {
        let k = v8::String::new(scope, name).unwrap();
        obj.set(scope, k.into(), noop.into());
    }
    rv.set(obj.into());
}

pub(super) fn element_get_animations(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    rv.set(v8::Array::new(scope, 0).into());
}

pub(super) fn element_after(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_mut(scope);
    let next = arena.nodes[node_id].next_sibling;
    let parent = arena.nodes[node_id].parent;

    for i in 0..args.length() {
        let arg = args.get(i);
        let new_id = if arg.is_string() {
            let text = arg.to_rust_string_lossy(scope);
            arena.new_node(NodeData::Text(text))
        } else if arg.is_object() {
            let obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(arg) };
            let Some(id) = unwrap_node_id(scope, obj) else { continue };
            if arena.nodes[id].parent.is_some() { arena.detach(id); }
            id
        } else {
            continue;
        };

        if let Some(next_id) = next {
            arena.insert_before(next_id, new_id);
        } else if let Some(parent_id) = parent {
            arena.append_child(parent_id, new_id);
        }
    }
}

pub(super) fn element_before(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_mut(scope);

    for i in 0..args.length() {
        let arg = args.get(i);
        let new_id = if arg.is_string() {
            let text = arg.to_rust_string_lossy(scope);
            arena.new_node(NodeData::Text(text))
        } else if arg.is_object() {
            let obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(arg) };
            let Some(id) = unwrap_node_id(scope, obj) else { continue };
            if arena.nodes[id].parent.is_some() { arena.detach(id); }
            id
        } else {
            continue;
        };

        arena.insert_before(node_id, new_id);
    }
}

pub(super) fn element_replace_with(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_mut(scope);
    let parent = arena.nodes[node_id].parent;

    // Insert all new nodes before self, then remove self
    for i in 0..args.length() {
        let arg = args.get(i);
        let new_id = if arg.is_string() {
            let text = arg.to_rust_string_lossy(scope);
            arena.new_node(NodeData::Text(text))
        } else if arg.is_object() {
            let obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(arg) };
            let Some(id) = unwrap_node_id(scope, obj) else { continue };
            if arena.nodes[id].parent.is_some() { arena.detach(id); }
            id
        } else {
            continue;
        };

        arena.insert_before(node_id, new_id);
    }
    if parent.is_some() {
        arena.detach(node_id);
    }
}

pub(super) fn insert_adjacent_text(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let position = args.get(0).to_rust_string_lossy(scope).to_ascii_lowercase();
    let text = args.get(1).to_rust_string_lossy(scope);

    let arena = arena_mut(scope);
    let text_node = arena.new_node(NodeData::Text(text));

    match position.as_str() {
        "beforebegin" => arena.insert_before(node_id, text_node),
        "afterbegin" => {
            if let Some(fc) = arena.nodes[node_id].first_child {
                arena.insert_before(fc, text_node);
            } else {
                arena.append_child(node_id, text_node);
            }
        }
        "beforeend" => arena.append_child(node_id, text_node),
        "afterend" => {
            if let Some(next) = arena.nodes[node_id].next_sibling {
                arena.insert_before(next, text_node);
            } else if let Some(parent) = arena.nodes[node_id].parent {
                arena.append_child(parent, text_node);
            }
        }
        _ => {}
    }
}
