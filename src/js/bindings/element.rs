/// Element prototype bindings.
///
/// Installs accessors and methods specific to the Element interface
/// (tagName, id, className, getAttribute/setAttribute, innerHTML, etc.)
/// Sub-concerns split into element_classlist, element_dataset, element_geometry.

use crate::dom::node::NodeData;
use crate::js::templates::{arena_mut, arena_ref, unwrap_node_id, wrap_node};
use super::helpers::{set_accessor, set_accessor_with_setter, set_method};
use super::element_classlist::class_list_getter;
use super::element_dataset::dataset_getter;
use super::element_geometry::{
    insert_adjacent_html, insert_adjacent_element, insert_adjacent_text,
    get_bounding_client_rect, geometry_zero_getter, element_noop,
    get_client_rects, get_attribute_names, has_attributes, toggle_attribute,
    get_attribute_node, attach_shadow, element_animate, element_get_animations,
    element_after, element_before, element_replace_with,
};

pub fn install(scope: &mut v8::HandleScope<()>, proto: &v8::Local<v8::ObjectTemplate>) {
    // Readonly accessors
    set_accessor(scope, proto, "tagName", tag_name_getter);
    set_accessor(scope, proto, "localName", local_name_getter);
    set_accessor(scope, proto, "outerHTML", outer_html_getter);
    set_accessor(scope, proto, "children", children_getter);
    set_accessor(scope, proto, "childElementCount", child_element_count_getter);
    set_accessor(scope, proto, "firstElementChild", first_element_child_getter);
    set_accessor(scope, proto, "lastElementChild", last_element_child_getter);
    set_accessor(scope, proto, "nextElementSibling", next_element_sibling_getter);
    set_accessor(scope, proto, "previousElementSibling", previous_element_sibling_getter);
    set_accessor(scope, proto, "namespaceURI", namespace_uri_getter);
    set_accessor(scope, proto, "prefix", null_getter);
    set_accessor(scope, proto, "attributes", attributes_getter);

    // Read-write accessors
    set_accessor_with_setter(scope, proto, "id", id_getter, id_setter);
    set_accessor_with_setter(scope, proto, "className", class_name_getter, class_name_setter);
    set_accessor_with_setter(scope, proto, "innerHTML", inner_html_getter, inner_html_setter);
    set_accessor_with_setter(scope, proto, "innerText", inner_text_getter, inner_text_setter);
    set_accessor(scope, proto, "outerText", inner_text_getter); // same as innerText getter
    set_accessor_with_setter(scope, proto, "hidden", hidden_getter, hidden_setter);
    set_accessor_with_setter(scope, proto, "tabIndex", tab_index_getter, tab_index_setter);

    // HTML reflecting IDL attributes (reflect content attributes as properties)
    set_accessor_with_setter(scope, proto, "src", reflecting_src_getter, reflecting_src_setter);
    set_accessor_with_setter(scope, proto, "href", reflecting_href_getter, reflecting_href_setter);
    set_accessor_with_setter(scope, proto, "value", reflecting_value_getter, reflecting_value_setter);
    set_accessor_with_setter(scope, proto, "type", reflecting_type_getter, reflecting_type_setter);
    set_accessor_with_setter(scope, proto, "name", reflecting_name_getter, reflecting_name_setter);
    set_accessor_with_setter(scope, proto, "disabled", reflecting_disabled_getter, reflecting_disabled_setter);
    set_accessor_with_setter(scope, proto, "checked", reflecting_checked_getter, reflecting_checked_setter);
    set_accessor_with_setter(scope, proto, "placeholder", reflecting_placeholder_getter, reflecting_placeholder_setter);

    // Batch 5: slot / assignedSlot
    set_accessor(scope, proto, "slot", empty_string_getter);
    set_accessor(scope, proto, "assignedSlot", null_getter);

    // Methods
    set_method(scope, proto, "getAttribute", get_attribute);
    set_method(scope, proto, "getAttributeNS", get_attribute_ns);
    set_method(scope, proto, "setAttribute", set_attribute);
    set_method(scope, proto, "setAttributeNS", set_attribute_ns);
    set_method(scope, proto, "removeAttribute", remove_attribute);
    set_method(scope, proto, "removeAttributeNS", remove_attribute_ns);
    set_method(scope, proto, "hasAttribute", has_attribute);
    set_method(scope, proto, "hasAttributeNS", has_attribute_ns);
    set_method(scope, proto, "remove", remove);
    set_method(scope, proto, "matches", matches_selector);
    set_method(scope, proto, "closest", closest_selector);
    set_method(scope, proto, "querySelector", element_query_selector);
    set_method(scope, proto, "querySelectorAll", element_query_selector_all);
    set_method(scope, proto, "getElementsByTagName", element_get_elements_by_tag_name);
    set_method(scope, proto, "getElementsByClassName", element_get_elements_by_class_name);
    set_method(scope, proto, "insertAdjacentHTML", insert_adjacent_html);
    set_method(scope, proto, "insertAdjacentElement", insert_adjacent_element);
    set_method(scope, proto, "insertAdjacentText", insert_adjacent_text);
    set_method(scope, proto, "getBoundingClientRect", get_bounding_client_rect);

    // Batch 4: New element methods
    set_method(scope, proto, "focus", element_noop);
    set_method(scope, proto, "blur", element_noop);
    set_method(scope, proto, "click", element_noop);
    set_method(scope, proto, "scrollIntoView", element_noop);
    set_method(scope, proto, "getClientRects", get_client_rects);
    set_method(scope, proto, "getAttributeNames", get_attribute_names);
    set_method(scope, proto, "hasAttributes", has_attributes);
    set_method(scope, proto, "toggleAttribute", toggle_attribute);
    set_method(scope, proto, "getAttributeNode", get_attribute_node);
    set_method(scope, proto, "attachShadow", attach_shadow);
    set_method(scope, proto, "animate", element_animate);
    set_method(scope, proto, "getAnimations", element_get_animations);
    set_method(scope, proto, "after", element_after);
    set_method(scope, proto, "before", element_before);
    set_method(scope, proto, "replaceWith", element_replace_with);

    // classList accessor
    set_accessor(scope, proto, "classList", class_list_getter);

    // style accessor
    super::style::install(scope, proto);

    // dataset accessor
    set_accessor(scope, proto, "dataset", dataset_getter);

    // Geometry stubs (all return 0)
    for name in &[
        "offsetWidth", "offsetHeight", "scrollWidth", "scrollHeight",
        "clientWidth", "clientHeight", "offsetTop", "offsetLeft",
        "scrollTop", "scrollLeft", "offsetParent",
    ] {
        set_accessor(scope, proto, name, geometry_zero_getter);
    }
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
        // Per DOM spec: "HTML-uppercased qualified name" only uppercases
        // elements in the HTML namespace. SVG/MathML/etc preserve case.
        let tag = if data.name.ns == markup5ever::ns!(html) {
            data.name.local.to_ascii_uppercase()
        } else {
            data.name.local.to_string().into()
        };
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
        let old = data.get_attribute("id").map(|s| s.to_string());
        data.set_attribute("id", &val);
        crate::js::mutation_observer::notify_attribute(scope, node_id, "id", old.as_deref());
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
        let old = data.get_attribute("class").map(|s| s.to_string());
        data.set_attribute("class", &val);
        crate::js::mutation_observer::notify_attribute(scope, node_id, "class", old.as_deref());
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

    // Capture old children for MO
    let old_children: Vec<crate::dom::NodeId> = arena.children(node_id).collect();

    // Remove existing children
    arena.remove_all_children(node_id);

    let mut new_children = Vec::new();
    if !html.is_empty() {
        // Parse fragment into a temporary arena
        let fragment_arena = crate::dom::treesink::parse_fragment(&html, &tag, true);

        // Transfer nodes from fragment arena into main arena.
        if let Some(html_wrapper) = fragment_arena.children(fragment_arena.document).next() {
            for child in fragment_arena.children(html_wrapper) {
                let new_id = clone_across_arenas(arena, &fragment_arena, child);
                arena.append_child(node_id, new_id);
                new_children.push(new_id);
            }
        }
    }

    if !old_children.is_empty() || !new_children.is_empty() {
        crate::js::mutation_observer::notify_child_list(
            scope, node_id, &new_children, &old_children, None, None,
        );
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
        let old_value = data.get_attribute(&attr_name).map(|s| s.to_string());
        data.set_attribute(&attr_name, &attr_value);
        crate::js::mutation_observer::notify_attribute(
            scope, node_id, &attr_name, old_value.as_deref(),
        );
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
        let old_value = data.get_attribute(&attr_name).map(|s| s.to_string());
        data.remove_attribute(&attr_name);
        if old_value.is_some() {
            crate::js::mutation_observer::notify_attribute(
                scope, node_id, &attr_name, old_value.as_deref(),
            );
        }
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

fn get_attribute_ns(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    // getAttributeNS(namespace, localName) — for SSR, just match by localName
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let local_name = args.get(1).to_rust_string_lossy(scope);
    let arena = arena_ref(scope);
    if let NodeData::Element(data) = &arena.nodes[node_id].data {
        // Search attributes by local name (ignore namespace for simplicity)
        for attr in &data.attrs {
            if &*attr.name.local == &*local_name {
                let v8_str = v8::String::new(scope, &attr.value).unwrap();
                rv.set(v8_str.into());
                return;
            }
        }
        rv.set(v8::null(scope).into());
    }
}

fn set_attribute_ns(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    // setAttributeNS(namespace, qualifiedName, value)
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let qualified = args.get(1).to_rust_string_lossy(scope);
    let value = args.get(2).to_rust_string_lossy(scope);

    // Extract local name from qualified name (e.g., "xlink:href" → "href")
    let local_name = if let Some(idx) = qualified.find(':') {
        &qualified[idx + 1..]
    } else {
        &qualified
    };

    let arena = arena_mut(scope);
    if let NodeData::Element(data) = &mut arena.nodes[node_id].data {
        data.set_attribute(local_name, &value);
    }
}

fn remove_attribute_ns(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let local_name = args.get(1).to_rust_string_lossy(scope);
    let arena = arena_mut(scope);
    if let NodeData::Element(data) = &mut arena.nodes[node_id].data {
        data.remove_attribute(&local_name);
    }
}

fn has_attribute_ns(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let local_name = args.get(1).to_rust_string_lossy(scope);
    let arena = arena_ref(scope);
    if let NodeData::Element(data) = &arena.nodes[node_id].data {
        let has = data.attrs.iter().any(|a| &*a.name.local == &*local_name);
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

fn matches_selector(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let selector = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_ref(scope);
    match crate::dom::selector::matches_element(arena, node_id, &selector) {
        Ok(matched) => rv.set(v8::Boolean::new(scope, matched).into()),
        Err(e) => {
            let msg = v8::String::new(scope, &e).unwrap();
            let exc = v8::Exception::syntax_error(scope, msg);
            scope.throw_exception(exc);
        }
    }
}

fn closest_selector(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let selector = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_ref(scope);
    match crate::dom::selector::closest(arena, node_id, &selector) {
        Ok(Some(id)) => rv.set(wrap_node(scope, id).into()),
        Ok(None) => rv.set(v8::null(scope).into()),
        Err(e) => {
            let msg = v8::String::new(scope, &e).unwrap();
            let exc = v8::Exception::syntax_error(scope, msg);
            scope.throw_exception(exc);
        }
    }
}

fn element_query_selector(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let selector = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_ref(scope);
    match crate::dom::selector::query_selector(arena, node_id, &selector) {
        Ok(Some(id)) => rv.set(wrap_node(scope, id).into()),
        Ok(None) | Err(_) => rv.set(v8::null(scope).into()),
    }
}

fn element_query_selector_all(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let selector = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_ref(scope);
    match crate::dom::selector::query_selector_all(arena, node_id, &selector) {
        Ok(ids) => {
            let arr = v8::Array::new(scope, ids.len() as i32);
            for (i, id) in ids.iter().enumerate() {
                let wrapped = wrap_node(scope, *id);
                arr.set_index(scope, i as u32, wrapped.into());
            }
            rv.set(arr.into());
        }
        Err(_e) => {
            // Lenient: return empty array instead of throwing on parse error
            let arr = v8::Array::new(scope, 0);
            rv.set(arr.into());
        }
    }
}

fn element_get_elements_by_tag_name(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let tag = args.get(0).to_rust_string_lossy(scope).to_ascii_lowercase();
    let arena = arena_ref(scope);
    let mut results = Vec::new();
    collect_elements_by_tag(arena, node_id, &tag, &mut results);
    let arr = v8::Array::new(scope, results.len() as i32);
    for (i, id) in results.iter().enumerate() {
        let wrapped = wrap_node(scope, *id);
        arr.set_index(scope, i as u32, wrapped.into());
    }
    rv.set(arr.into());
}

fn element_get_elements_by_class_name(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let class_names = args.get(0).to_rust_string_lossy(scope);
    let wanted: Vec<&str> = class_names.split_whitespace().collect();
    if wanted.is_empty() {
        let arr = v8::Array::new(scope, 0);
        rv.set(arr.into());
        return;
    }
    let arena = arena_ref(scope);
    let mut results = Vec::new();
    collect_elements_by_class(arena, node_id, &wanted, &mut results);
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

fn serialize_node(arena: &crate::dom::Arena, id: crate::dom::NodeId, output: &mut String) {
    crate::dom::serialize::serialize_node_to_string(arena, id, output);
}

// ─── Batch 4: New element functions ───────────────────────────────────────────

fn local_name_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    if let NodeData::Element(data) = &arena.nodes[node_id].data {
        let v8_str = v8::String::new(scope, &data.name.local).unwrap();
        rv.set(v8_str.into());
    }
}

fn namespace_uri_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    if let NodeData::Element(data) = &arena.nodes[node_id].data {
        let uri = if data.name.ns == markup5ever::ns!(html) {
            "http://www.w3.org/1999/xhtml"
        } else if data.name.ns == markup5ever::ns!(svg) {
            "http://www.w3.org/2000/svg"
        } else if data.name.ns == markup5ever::ns!(mathml) {
            "http://www.w3.org/1998/Math/MathML"
        } else {
            &*data.name.ns
        };
        rv.set(v8::String::new(scope, uri).unwrap().into());
    }
}

fn null_getter(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    rv.set(v8::null(scope).into());
}

fn empty_string_getter(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    rv.set(v8::String::new(scope, "").unwrap().into());
}

fn attributes_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    if let NodeData::Element(data) = &arena.nodes[node_id].data {
        let obj = v8::Object::new(scope);
        let mut i = 0;
        for attr in &data.attrs {
            let name = &*attr.name.local;
            let attr_obj = v8::Object::new(scope);
            let k = v8::String::new(scope, "name").unwrap();
            let v = v8::String::new(scope, name).unwrap();
            attr_obj.set(scope, k.into(), v.into());
            let k = v8::String::new(scope, "value").unwrap();
            let v = v8::String::new(scope, &attr.value).unwrap();
            attr_obj.set(scope, k.into(), v.into());
            let k = v8::String::new(scope, "specified").unwrap();
            let v = v8::Boolean::new(scope, true);
            attr_obj.set(scope, k.into(), v.into());
            // Set by name
            let k = v8::String::new(scope, name).unwrap();
            obj.set(scope, k.into(), attr_obj.into());
            // Set by index
            obj.set_index(scope, i, attr_obj.into());
            i += 1;
        }
        let k = v8::String::new(scope, "length").unwrap();
        let v = v8::Integer::new(scope, data.attrs.len() as i32);
        obj.set(scope, k.into(), v.into());
        // getNamedItem method
        let gni = v8::Function::new(scope, |scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
            let this = args.this();
            let name = args.get(0).to_rust_string_lossy(scope);
            let k = v8::String::new(scope, &name).unwrap();
            if let Some(val) = this.get(scope, k.into()) {
                if !val.is_undefined() {
                    rv.set(val);
                    return;
                }
            }
            rv.set(v8::null(scope).into());
        }).unwrap();
        let k = v8::String::new(scope, "getNamedItem").unwrap();
        obj.set(scope, k.into(), gni.into());
        rv.set(obj.into());
    }
}

fn inner_text_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    // Map to textContent (simplified — real innerText depends on layout)
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let mut text = String::new();
    collect_text_content(arena, node_id, &mut text);
    let v8_str = v8::String::new(scope, &text).unwrap();
    rv.set(v8_str.into());
}

fn collect_text_content(arena: &crate::dom::Arena, id: crate::dom::NodeId, out: &mut String) {
    match &arena.nodes[id].data {
        NodeData::Text(s) => out.push_str(s),
        NodeData::Element(data) => {
            // Skip script/style content
            let tag = &*data.name.local;
            if tag == "script" || tag == "style" {
                return;
            }
            for child in arena.children(id) {
                collect_text_content(arena, child, out);
            }
        }
        _ => {
            for child in arena.children(id) {
                collect_text_content(arena, child, out);
            }
        }
    }
}

fn inner_text_setter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    // Map to textContent setter
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let text = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_mut(scope);
    arena.remove_all_children(node_id);
    if !text.is_empty() {
        let text_node = arena.new_node(NodeData::Text(text));
        arena.append_child(node_id, text_node);
    }
}

fn hidden_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let has = if let NodeData::Element(data) = &arena.nodes[node_id].data {
        data.get_attribute("hidden").is_some()
    } else {
        false
    };
    rv.set(v8::Boolean::new(scope, has).into());
}

fn hidden_setter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let val = args.get(0).boolean_value(scope);
    let arena = arena_mut(scope);
    if let NodeData::Element(data) = &mut arena.nodes[node_id].data {
        if val {
            data.set_attribute("hidden", "");
        } else {
            data.remove_attribute("hidden");
        }
    }
}

fn tab_index_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let val = if let NodeData::Element(data) = &arena.nodes[node_id].data {
        data.get_attribute("tabindex")
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(-1)
    } else {
        -1
    };
    rv.set(v8::Integer::new(scope, val).into());
}

fn tab_index_setter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let val = args.get(0).int32_value(scope).unwrap_or(-1);
    let arena = arena_mut(scope);
    if let NodeData::Element(data) = &mut arena.nodes[node_id].data {
        data.set_attribute("tabindex", &val.to_string());
    }
}

// ─── Reflecting IDL attribute accessors ──────────────────────────────────────
//
// Per the HTML spec, many IDL attributes "reflect" a content attribute:
// el.src ↔ el.getAttribute("src") / el.setAttribute("src", val)
// These are DOMString reflecting attributes (return "" if absent).

macro_rules! reflecting_string_accessor {
    ($getter:ident, $setter:ident, $attr:literal) => {
        fn $getter(
            scope: &mut v8::HandleScope,
            args: v8::FunctionCallbackArguments,
            mut rv: v8::ReturnValue,
        ) {
            let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
            let arena = arena_ref(scope);
            if let NodeData::Element(data) = &arena.nodes[node_id].data {
                let val = data.get_attribute($attr).unwrap_or("");
                let v8_str = v8::String::new(scope, val).unwrap();
                rv.set(v8_str.into());
            }
        }

        fn $setter(
            scope: &mut v8::HandleScope,
            args: v8::FunctionCallbackArguments,
            _rv: v8::ReturnValue,
        ) {
            let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
            let val = args.get(0).to_rust_string_lossy(scope);
            let arena = arena_mut(scope);
            if let NodeData::Element(data) = &mut arena.nodes[node_id].data {
                data.set_attribute($attr, &val);
            }
        }
    };
}

reflecting_string_accessor!(reflecting_src_getter, reflecting_src_setter, "src");
reflecting_string_accessor!(reflecting_href_getter, reflecting_href_setter, "href");
reflecting_string_accessor!(reflecting_value_getter, reflecting_value_setter, "value");
reflecting_string_accessor!(reflecting_type_getter, reflecting_type_setter, "type");
reflecting_string_accessor!(reflecting_name_getter, reflecting_name_setter, "name");
reflecting_string_accessor!(reflecting_placeholder_getter, reflecting_placeholder_setter, "placeholder");

// Boolean reflecting attributes: return true if attribute present, false if absent
macro_rules! reflecting_boolean_accessor {
    ($getter:ident, $setter:ident, $attr:literal) => {
        fn $getter(
            scope: &mut v8::HandleScope,
            args: v8::FunctionCallbackArguments,
            mut rv: v8::ReturnValue,
        ) {
            let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
            let arena = arena_ref(scope);
            if let NodeData::Element(data) = &arena.nodes[node_id].data {
                let has = data.get_attribute($attr).is_some();
                rv.set(v8::Boolean::new(scope, has).into());
            }
        }

        fn $setter(
            scope: &mut v8::HandleScope,
            args: v8::FunctionCallbackArguments,
            _rv: v8::ReturnValue,
        ) {
            let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
            let val = args.get(0).boolean_value(scope);
            let arena = arena_mut(scope);
            if let NodeData::Element(data) = &mut arena.nodes[node_id].data {
                if val {
                    data.set_attribute($attr, "");
                } else {
                    data.remove_attribute($attr);
                }
            }
        }
    };
}

reflecting_boolean_accessor!(reflecting_disabled_getter, reflecting_disabled_setter, "disabled");
reflecting_boolean_accessor!(reflecting_checked_getter, reflecting_checked_setter, "checked");


pub fn clone_across_arenas(
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
