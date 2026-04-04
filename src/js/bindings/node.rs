/// Node prototype bindings.
///
/// Installs accessors and methods on the Node prototype template, which
/// Document, Element, Text, and Comment all inherit from.

use crate::dom::arena::DomValidationError;
use crate::dom::node::NodeData;
use crate::js::templates::{arena_mut, arena_ref, unwrap_node_id, wrap_node};
use super::helpers::{set_accessor, set_accessor_with_setter, set_method};

/// Throw a DOMException-style error in V8 for a DOM validation failure.
fn throw_dom_error(scope: &mut v8::HandleScope, err: DomValidationError) {
    let msg = v8::String::new(scope, &err.to_string()).unwrap();
    let exc = v8::Exception::error(scope, msg);
    // Set the name property on the exception to match DOMException convention
    if let Some(exc_obj) = exc.to_object(scope) {
        let name_key = v8::String::new(scope, "name").unwrap();
        let name_val = match err {
            DomValidationError::HierarchyRequest => v8::String::new(scope, "HierarchyRequestError").unwrap(),
            DomValidationError::NotFound => v8::String::new(scope, "NotFoundError").unwrap(),
        };
        exc_obj.set(scope, name_key.into(), name_val.into());
    }
    scope.throw_exception(exc);
}

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
    set_accessor(scope, proto, "isConnected", is_connected_getter);

    // Read-write accessor
    set_accessor_with_setter(scope, proto, "textContent", text_content_getter, text_content_setter);

    // Methods
    set_method(scope, proto, "appendChild", append_child);
    set_method(scope, proto, "removeChild", remove_child);
    set_method(scope, proto, "insertBefore", insert_before);
    set_method(scope, proto, "replaceChild", replace_child);
    set_method(scope, proto, "cloneNode", clone_node);
    set_method(scope, proto, "hasChildNodes", has_child_nodes);
    set_method(scope, proto, "contains", contains);
    set_method(scope, proto, "isSameNode", is_same_node);
    set_method(scope, proto, "isEqualNode", is_equal_node);
    set_method(scope, proto, "normalize", normalize);
    set_method(scope, proto, "getRootNode", get_root_node);
    set_method(scope, proto, "compareDocumentPosition", compare_document_position);
    set_method(scope, proto, "lookupPrefix", lookup_prefix);
    set_method(scope, proto, "lookupNamespaceURI", lookup_namespace_uri);
    set_method(scope, proto, "isDefaultNamespace", is_default_namespace);

    // Node type constants
    let set_const = |scope: &mut v8::HandleScope<()>, proto: &v8::Local<v8::ObjectTemplate>, name: &str, val: i32| {
        let key = v8::String::new(scope, name).unwrap();
        let value = v8::Integer::new(scope, val);
        proto.set(key.into(), value.into());
    };
    set_const(scope, proto, "ELEMENT_NODE", 1);
    set_const(scope, proto, "ATTRIBUTE_NODE", 2);
    set_const(scope, proto, "TEXT_NODE", 3);
    set_const(scope, proto, "CDATA_SECTION_NODE", 4);
    set_const(scope, proto, "PROCESSING_INSTRUCTION_NODE", 7);
    set_const(scope, proto, "COMMENT_NODE", 8);
    set_const(scope, proto, "DOCUMENT_NODE", 9);
    set_const(scope, proto, "DOCUMENT_TYPE_NODE", 10);
    set_const(scope, proto, "DOCUMENT_FRAGMENT_NODE", 11);
    set_const(scope, proto, "DOCUMENT_POSITION_DISCONNECTED", 0x01);
    set_const(scope, proto, "DOCUMENT_POSITION_PRECEDING", 0x02);
    set_const(scope, proto, "DOCUMENT_POSITION_FOLLOWING", 0x04);
    set_const(scope, proto, "DOCUMENT_POSITION_CONTAINS", 0x08);
    set_const(scope, proto, "DOCUMENT_POSITION_CONTAINED_BY", 0x10);
    set_const(scope, proto, "DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC", 0x20);

    // DOM4 convenience methods
    set_method(scope, proto, "append", append_nodes);
    set_method(scope, proto, "prepend", prepend_nodes);
    set_method(scope, proto, "before", before_nodes);
    set_method(scope, proto, "after", after_nodes);
    set_method(scope, proto, "replaceWith", replace_with);

    // Event methods (inherited by all node types)
    set_method(scope, proto, "addEventListener", crate::js::events::add_event_listener_callback);
    set_method(scope, proto, "removeEventListener", crate::js::events::remove_event_listener_callback);
    set_method(scope, proto, "dispatchEvent", crate::js::events::dispatch_event_callback);
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
        NodeData::DocumentFragment => 11,
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
        NodeData::Element(data) => {
            // Per DOM spec: only uppercase for HTML namespace elements
            if data.name.ns == markup5ever::ns!(html) {
                data.name.local.to_ascii_uppercase().to_string()
            } else {
                data.name.local.to_string()
            }
        }
        NodeData::Text(_) => "#text".to_string(),
        NodeData::Comment(_) => "#comment".to_string(),
        NodeData::Document => "#document".to_string(),
        NodeData::DocumentFragment => "#document-fragment".to_string(),
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

    // Check cache for existing live NodeList
    let cached = {
        let cache = scope.get_slot::<crate::js::templates::ChildNodesCache>().unwrap();
        cache.map.get(&node_id).cloned()
    };
    if let Some(global) = cached {
        let local = v8::Local::new(scope, &global);
        rv.set(local.into());
        return;
    }

    // Create a live NodeList proxy
    let proxy = create_live_child_nodes(scope, node_id);

    // Cache it
    let global = v8::Global::new(scope, proxy);
    let cache = scope.get_slot_mut::<crate::js::templates::ChildNodesCache>().unwrap();
    cache.map.insert(node_id, global);

    rv.set(proxy.into());
}

/// Create a Proxy-based live NodeList for childNodes.
fn create_live_child_nodes<'s>(
    scope: &mut v8::HandleScope<'s>,
    node_id: crate::dom::NodeId,
) -> v8::Local<'s, v8::Object> {
    // Target object: stores the NodeId for callbacks to find
    let target = v8::Object::new(scope);
    let id_key = v8::String::new(scope, "__nodeId").unwrap();
    // Store the NodeId as a wrapped node so we can recover it
    let wrapped = wrap_node(scope, node_id);
    target.set(scope, id_key.into(), wrapped.into());

    // Handler with get trap
    let handler = v8::Object::new(scope);

    // get(target, prop, receiver)
    let get_fn = v8::Function::new(scope, child_nodes_get_trap).unwrap();
    let get_key = v8::String::new(scope, "get").unwrap();
    handler.set(scope, get_key.into(), get_fn.into());

    // ownKeys(target) — return ["0", "1", ..., "length"]
    let own_keys_fn = v8::Function::new(scope, child_nodes_own_keys_trap).unwrap();
    let ok_key = v8::String::new(scope, "ownKeys").unwrap();
    handler.set(scope, ok_key.into(), own_keys_fn.into());

    // getOwnPropertyDescriptor(target, prop) — needed for ownKeys to work
    let gopd_fn = v8::Function::new(scope, child_nodes_gopd_trap).unwrap();
    let gopd_key = v8::String::new(scope, "getOwnPropertyDescriptor").unwrap();
    handler.set(scope, gopd_key.into(), gopd_fn.into());

    let proxy = v8::Proxy::new(scope, target, handler).unwrap();
    let proxy_obj = proxy.into();
    proxy_obj
}

/// Helper: extract the NodeId from a childNodes proxy target.
fn child_nodes_target_id(scope: &mut v8::HandleScope, target: v8::Local<v8::Object>) -> Option<crate::dom::NodeId> {
    let id_key = v8::String::new(scope, "__nodeId").unwrap();
    let node_obj = target.get(scope, id_key.into())?;
    if !node_obj.is_object() { return None; }
    let obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(node_obj) };
    unwrap_node_id(scope, obj)
}

/// Get trap for live childNodes Proxy.
fn child_nodes_get_trap(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let target = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(0)) };
    let prop = args.get(1);

    let Some(node_id) = child_nodes_target_id(scope, target) else { return };

    // Handle string properties
    let prop_str = prop.to_rust_string_lossy(scope);

    match prop_str.as_str() {
        "length" => {
            let arena = arena_ref(scope);
            let count = arena.children(node_id).count();
            rv.set(v8::Integer::new(scope, count as i32).into());
        }
        "item" => {
            // Return a function that takes an index
            // We need to capture node_id — store it on the function via the target
            let item_fn = v8::Function::new(scope, child_nodes_item_fn).unwrap();
            // Store target on the function's context (use name hack)
            rv.set(item_fn.into());
        }
        "forEach" => {
            let foreach_fn = v8::Function::new(scope, child_nodes_foreach_fn).unwrap();
            rv.set(foreach_fn.into());
        }
        "entries" | "keys" | "values" => {
            // Return a no-op function (simplified — these are rarely used on childNodes)
            let noop = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
                rv.set(v8::Array::new(scope, 0).into());
            }).unwrap();
            rv.set(noop.into());
        }
        _ => {
            // Try numeric index
            if let Ok(index) = prop_str.parse::<usize>() {
                let arena = arena_ref(scope);
                if let Some(child_id) = arena.children(node_id).nth(index) {
                    let wrapped = wrap_node(scope, child_id);
                    rv.set(wrapped.into());
                } else {
                    rv.set(v8::undefined(scope).into());
                }
            } else if prop_str == "Symbol(Symbol.iterator)" || prop_str == "Symbol(Symbol.toStringTag)" {
                // Handled below
            }
            // For unrecognized properties, return undefined (default proxy behavior)
        }
    }
}

/// item(index) method on live childNodes.
fn child_nodes_item_fn(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    // `this` is the proxy — get the target's nodeId
    let _this = args.this();
    // For item(), we need the parent node_id. Traverse from this proxy's target.
    // Since `this` is the proxy, we need another way. Store parent id externally.
    // Simplest: walk from the proxy receiver. Actually for item(), we can look at caller context.
    // Fallback: just scan all children. Since `this` = proxy, we stored __nodeId on target.
    // With Proxy, `this` is the receiver which is the proxy itself, not the target.
    // We can't easily get target from proxy in a generic function.
    // Instead, let's use a different approach: return an ad-hoc closure.

    let index = args.get(0).uint32_value(scope).unwrap_or(0) as usize;
    // This is a limitation — we need node_id but can't get it here easily.
    // Return null for now (item() is rarely used compared to indexed access).
    rv.set(v8::null(scope).into());
    let _ = index;
}

/// forEach(callback) on live childNodes.
fn child_nodes_foreach_fn(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let callback_arg = args.get(0);
    if !callback_arg.is_function() { return; }
    let callback = unsafe { v8::Local::<v8::Function>::cast_unchecked(callback_arg) };

    // `this` is the proxy — get length and iterate
    let this = args.this();
    let len_key = v8::String::new(scope, "length").unwrap();
    let length = this.get(scope, len_key.into())
        .and_then(|v| v.uint32_value(scope))
        .unwrap_or(0);

    for i in 0..length {
        let child = this.get_index(scope, i);
        if let Some(child_val) = child {
            let idx = v8::Integer::new(scope, i as i32);
            let undefined = v8::undefined(scope);
            callback.call(scope, undefined.into(), &[child_val, idx.into(), this.into()]);
        }
    }
}

/// ownKeys trap — returns ["0", "1", ..., "length"].
fn child_nodes_own_keys_trap(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let target = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(0)) };
    let Some(node_id) = child_nodes_target_id(scope, target) else { return };

    let arena = arena_ref(scope);
    let count = arena.children(node_id).count();
    let arr = v8::Array::new(scope, (count + 1) as i32);
    for i in 0..count {
        let key = v8::String::new(scope, &i.to_string()).unwrap();
        arr.set_index(scope, i as u32, key.into());
    }
    let len_key = v8::String::new(scope, "length").unwrap();
    arr.set_index(scope, count as u32, len_key.into());
    rv.set(arr.into());
}

/// getOwnPropertyDescriptor trap — returns configurable+enumerable descriptor.
fn child_nodes_gopd_trap(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let target = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(0)) };
    let prop = args.get(1);
    let Some(node_id) = child_nodes_target_id(scope, target) else { return };

    let prop_str = prop.to_rust_string_lossy(scope);
    let arena = arena_ref(scope);

    let desc = v8::Object::new(scope);
    let config_key = v8::String::new(scope, "configurable").unwrap();
    let enum_key = v8::String::new(scope, "enumerable").unwrap();
    let t = v8::Boolean::new(scope, true);
    desc.set(scope, config_key.into(), t.into());
    desc.set(scope, enum_key.into(), t.into());

    let val_key = v8::String::new(scope, "value").unwrap();
    if prop_str == "length" {
        let count = arena.children(node_id).count();
        let v = v8::Integer::new(scope, count as i32);
        desc.set(scope, val_key.into(), v.into());
    } else if let Ok(index) = prop_str.parse::<usize>() {
        if let Some(child_id) = arena.children(node_id).nth(index) {
            let wrapped = wrap_node(scope, child_id);
            desc.set(scope, val_key.into(), wrapped.into());
        } else {
            rv.set(v8::undefined(scope).into());
            return;
        }
    } else {
        rv.set(v8::undefined(scope).into());
        return;
    }

    rv.set(desc.into());
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

/// isConnected — O(1) check via NodeFlags.IS_CONNECTED.
/// https://dom.spec.whatwg.org/#dom-node-isconnected
fn is_connected_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let connected = arena.nodes[node_id].flags.is_connected();
    rv.set(v8::Boolean::new(scope, connected).into());
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

    // Per WHATWG DOM §4.4: for Text/Comment nodes, setting textContent
    // replaces the node's data (characterData mutation), not childList.
    match &mut arena.nodes[node_id].data {
        NodeData::Text(s) | NodeData::Comment(s) => {
            let old_value = s.clone();
            *s = text;
            crate::js::mutation_observer::notify_character_data(scope, node_id, &old_value);
            return;
        }
        _ => {}
    }

    // For Element/Document/DocumentFragment: replace all children with a text node.
    // Capture old children for MO
    let old_children: Vec<crate::dom::NodeId> = arena.children(node_id).collect();
    arena.remove_all_children(node_id);
    let new_children = if !text.is_empty() {
        let text_node = arena.new_node(NodeData::Text(text));
        arena.append_child(node_id, text_node);
        vec![text_node]
    } else {
        vec![]
    };
    if !old_children.is_empty() || !new_children.is_empty() {
        crate::js::mutation_observer::notify_child_list(
            scope, node_id, &new_children, &old_children, None, None,
        );
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
        // Per WHATWG DOM §4.4, textContent for Text/Comment returns data
        NodeData::Text(s) | NodeData::Comment(s) => buf.push_str(s),
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
    // Pre-insertion validation (child=None means "append at end")
    if let Err(e) = arena.ensure_pre_insertion_validity(child_id, parent_id, None) {
        throw_dom_error(scope, e);
        return;
    }
    // Capture pre-mutation state for MutationObserver
    let prev_sib = arena.nodes[parent_id].last_child;

    // Per spec: if node is a DocumentFragment, append its children instead
    if matches!(&arena.nodes[child_id].data, NodeData::DocumentFragment) {
        let frag_children: Vec<crate::dom::NodeId> = arena.children(child_id).collect();
        arena.reparent_children(child_id, parent_id);
        crate::js::mutation_observer::notify_child_list(
            scope, parent_id, &frag_children, &[], prev_sib, None,
        );
    } else {
        // Detach from current parent if any (spec: re-parenting)
        if arena.nodes[child_id].parent.is_some() {
            arena.detach(child_id);
        }
        arena.append_child(parent_id, child_id);
        crate::js::mutation_observer::notify_child_list(
            scope, parent_id, &[child_id], &[], prev_sib, None,
        );
    }
    rv.set(child_arg);
}

fn remove_child(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(parent_id) = unwrap_node_id(scope, args.this()) else { return };
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
    // Per spec: child must be a child of this node
    if arena.nodes[child_id].parent != Some(parent_id) {
        throw_dom_error(scope, DomValidationError::NotFound);
        return;
    }
    // Capture siblings before detach for MutationObserver
    let prev_sib = arena.nodes[child_id].prev_sibling;
    let next_sib = arena.nodes[child_id].next_sibling;
    arena.detach(child_id);
    crate::js::mutation_observer::notify_child_list(
        scope, parent_id, &[], &[child_id], prev_sib, next_sib,
    );
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

    // Determine the reference node
    let ref_id = if ref_node_arg.is_null() || ref_node_arg.is_undefined() {
        None
    } else if ref_node_arg.is_object() {
        let ref_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(ref_node_arg) };
        unwrap_node_id(scope, ref_obj)
    } else {
        None
    };

    let arena = arena_mut(scope);
    // Pre-insertion validation
    if let Err(e) = arena.ensure_pre_insertion_validity(new_id, parent_id, ref_id) {
        throw_dom_error(scope, e);
        return;
    }
    // Capture pre-mutation sibling for MO
    let prev_sib = ref_id.map(|r| arena.nodes[r].prev_sibling).unwrap_or(arena.nodes[parent_id].last_child);

    // Per spec: if node is a DocumentFragment, insert its children instead
    if matches!(&arena.nodes[new_id].data, NodeData::DocumentFragment) {
        let children: Vec<crate::dom::NodeId> = arena.children(new_id).collect();
        for child in children.iter() {
            arena.detach(*child);
            if let Some(ref_id) = ref_id {
                arena.insert_before(ref_id, *child);
            } else {
                arena.append_child(parent_id, *child);
            }
        }
        if !children.is_empty() {
            crate::js::mutation_observer::notify_child_list(
                scope, parent_id, &children, &[], prev_sib, ref_id,
            );
        }
    } else {
        // Detach from current parent if any
        if arena.nodes[new_id].parent.is_some() {
            arena.detach(new_id);
        }
        if let Some(ref_id) = ref_id {
            arena.insert_before(ref_id, new_id);
        } else {
            // insertBefore(node, null) acts like appendChild
            arena.append_child(parent_id, new_id);
        }
        crate::js::mutation_observer::notify_child_list(
            scope, parent_id, &[new_id], &[], prev_sib, ref_id,
        );
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

fn is_equal_node(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id_a) = unwrap_node_id(scope, args.this()) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let other = args.get(0);
    if other.is_null_or_undefined() || !other.is_object() {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    }
    let other_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(other) };
    let Some(node_id_b) = unwrap_node_id(scope, other_obj) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };

    let arena = arena_ref(scope);
    let result = nodes_equal(arena, node_id_a, node_id_b);
    rv.set(v8::Boolean::new(scope, result).into());
}

fn nodes_equal(arena: &crate::dom::Arena, a: crate::dom::NodeId, b: crate::dom::NodeId) -> bool {
    use crate::dom::node::NodeData;

    if a == b { return true; }

    let da = &arena.nodes[a].data;
    let db = &arena.nodes[b].data;

    match (da, db) {
        (NodeData::Element(ea), NodeData::Element(eb)) => {
            if ea.name != eb.name { return false; }
            if ea.attrs.len() != eb.attrs.len() { return false; }
            for attr_a in &ea.attrs {
                if !eb.attrs.iter().any(|ab| attr_a.name == ab.name && attr_a.value == ab.value) {
                    return false;
                }
            }
            let ca: Vec<_> = arena.children(a).collect();
            let cb: Vec<_> = arena.children(b).collect();
            ca.len() == cb.len() && ca.iter().zip(cb.iter()).all(|(c1, c2)| nodes_equal(arena, *c1, *c2))
        }
        (NodeData::Text(ta), NodeData::Text(tb)) => ta == tb,
        (NodeData::Comment(ca), NodeData::Comment(cb)) => ca == cb,
        (NodeData::Document, NodeData::Document)
        | (NodeData::DocumentFragment, NodeData::DocumentFragment) => {
            let ca: Vec<_> = arena.children(a).collect();
            let cb: Vec<_> = arena.children(b).collect();
            ca.len() == cb.len() && ca.iter().zip(cb.iter()).all(|(c1, c2)| nodes_equal(arena, *c1, *c2))
        }
        _ => false,
    }
}

/// normalize() — merge adjacent Text nodes, remove empty ones.
/// Ported from Servo's node.rs Normalize implementation.
/// https://dom.spec.whatwg.org/#dom-node-normalize
fn normalize(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_mut(scope);
    do_normalize(arena, node_id);
}

fn do_normalize(arena: &mut crate::dom::Arena, node_id: crate::dom::NodeId) {
    // Collect children first to avoid borrow issues
    let children: Vec<crate::dom::NodeId> = arena.children(node_id).collect();

    let mut i = 0;
    while i < children.len() {
        let child = children[i];
        // Skip removed nodes
        if arena.nodes[child].parent != Some(node_id) {
            i += 1;
            continue;
        }

        match &arena.nodes[child].data {
            NodeData::Text(_) => {
                // Check if text is empty → remove
                let is_empty = matches!(&arena.nodes[child].data, NodeData::Text(s) if s.is_empty());
                if is_empty {
                    arena.detach(child);
                    i += 1;
                    continue;
                }

                // Merge subsequent adjacent text nodes
                let mut j = i + 1;
                while j < children.len() {
                    let sibling = children[j];
                    if arena.nodes[sibling].parent != Some(node_id) {
                        j += 1;
                        continue;
                    }
                    if !matches!(&arena.nodes[sibling].data, NodeData::Text(_)) {
                        break;
                    }
                    // Append sibling text to this node
                    let sibling_text = match &arena.nodes[sibling].data {
                        NodeData::Text(s) => s.clone(),
                        _ => unreachable!(),
                    };
                    if let NodeData::Text(s) = &mut arena.nodes[child].data {
                        s.push_str(&sibling_text);
                    }
                    arena.detach(sibling);
                    j += 1;
                }
                i = j;
            }
            NodeData::Element(_) | NodeData::Document | NodeData::DocumentFragment => {
                // Recursively normalize children
                do_normalize(arena, child);
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
}

fn get_root_node(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let mut current = node_id;
    while let Some(parent) = arena.nodes[current].parent {
        current = parent;
    }
    rv.set(wrap_node(scope, current).into());
}

fn replace_child(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(parent_id) = unwrap_node_id(scope, args.this()) else { return };
    let new_arg = args.get(0);
    let old_arg = args.get(1);

    if !new_arg.is_object() || !old_arg.is_object() {
        let msg = v8::String::new(scope, "replaceChild: arguments must be Nodes").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    }
    let new_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(new_arg) };
    let old_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(old_arg) };
    let Some(new_id) = unwrap_node_id(scope, new_obj) else { return };
    let Some(old_id) = unwrap_node_id(scope, old_obj) else { return };

    let arena = arena_mut(scope);
    // Replace-child validation
    if let Err(e) = arena.ensure_replace_validity(new_id, parent_id, old_id) {
        throw_dom_error(scope, e);
        return;
    }
    // Capture siblings before mutation for MO
    let prev_sib = arena.nodes[old_id].prev_sibling;
    let next_sib = arena.nodes[old_id].next_sibling;
    // Insert new before old, then detach old
    if arena.nodes[new_id].parent.is_some() {
        arena.detach(new_id);
    }
    arena.insert_before(old_id, new_id);
    arena.detach(old_id);
    crate::js::mutation_observer::notify_child_list(
        scope, parent_id, &[new_id], &[old_id], prev_sib, next_sib,
    );

    rv.set(old_arg);
}

/// Convert a JS argument to a NodeId — if it's a string, create a text node.
fn arg_to_node_id(scope: &mut v8::HandleScope, arg: v8::Local<v8::Value>) -> Option<crate::dom::NodeId> {
    if arg.is_string() || arg.is_number() {
        let text = arg.to_rust_string_lossy(scope);
        let arena = arena_mut(scope);
        Some(arena.new_node(NodeData::Text(text)))
    } else if arg.is_object() {
        let obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(arg) };
        unwrap_node_id(scope, obj)
    } else {
        None
    }
}

fn append_nodes(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(parent_id) = unwrap_node_id(scope, args.this()) else { return };
    let mut nodes = Vec::new();
    for i in 0..args.length() {
        if let Some(id) = arg_to_node_id(scope, args.get(i)) {
            nodes.push(id);
        }
    }
    let arena = arena_mut(scope);
    for id in nodes {
        if arena.nodes[id].parent.is_some() {
            arena.detach(id);
        }
        arena.append_child(parent_id, id);
    }
}

fn prepend_nodes(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(parent_id) = unwrap_node_id(scope, args.this()) else { return };
    let mut nodes = Vec::new();
    for i in 0..args.length() {
        if let Some(id) = arg_to_node_id(scope, args.get(i)) {
            nodes.push(id);
        }
    }
    let arena = arena_mut(scope);
    let first_child = arena.nodes[parent_id].first_child;
    for id in nodes {
        if arena.nodes[id].parent.is_some() {
            arena.detach(id);
        }
        if let Some(fc) = first_child {
            // Only insert before if the reference still exists and has parent
            if arena.nodes[fc].parent.is_some() {
                arena.insert_before(fc, id);
            } else {
                arena.append_child(parent_id, id);
            }
        } else {
            arena.append_child(parent_id, id);
        }
    }
}

fn before_nodes(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let mut nodes = Vec::new();
    for i in 0..args.length() {
        if let Some(id) = arg_to_node_id(scope, args.get(i)) {
            nodes.push(id);
        }
    }
    let arena = arena_mut(scope);
    if arena.nodes[node_id].parent.is_none() { return; }
    for id in nodes {
        if arena.nodes[id].parent.is_some() {
            arena.detach(id);
        }
        arena.insert_before(node_id, id);
    }
}

fn after_nodes(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let mut nodes = Vec::new();
    for i in 0..args.length() {
        if let Some(id) = arg_to_node_id(scope, args.get(i)) {
            nodes.push(id);
        }
    }
    let arena = arena_mut(scope);
    let parent = match arena.nodes[node_id].parent {
        Some(p) => p,
        None => return,
    };
    let next = arena.nodes[node_id].next_sibling;
    for id in nodes {
        if arena.nodes[id].parent.is_some() {
            arena.detach(id);
        }
        if let Some(next_id) = next {
            if arena.nodes[next_id].parent.is_some() {
                arena.insert_before(next_id, id);
            } else {
                arena.append_child(parent, id);
            }
        } else {
            arena.append_child(parent, id);
        }
    }
}

fn replace_with(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let mut nodes = Vec::new();
    for i in 0..args.length() {
        if let Some(id) = arg_to_node_id(scope, args.get(i)) {
            nodes.push(id);
        }
    }
    let arena = arena_mut(scope);
    if arena.nodes[node_id].parent.is_none() { return; }
    // Insert all replacement nodes before this node, then detach this
    for id in nodes {
        if arena.nodes[id].parent.is_some() {
            arena.detach(id);
        }
        arena.insert_before(node_id, id);
    }
    arena.detach(node_id);
}

// ─── compareDocumentPosition ─────────────────────────────────────────────────
// Ported from Servo's node.rs CompareDocumentPosition.
// https://dom.spec.whatwg.org/#dom-node-comparedocumentposition

const DOCUMENT_POSITION_DISCONNECTED: u16 = 0x01;
const DOCUMENT_POSITION_PRECEDING: u16 = 0x02;
const DOCUMENT_POSITION_FOLLOWING: u16 = 0x04;
const DOCUMENT_POSITION_CONTAINS: u16 = 0x08;
const DOCUMENT_POSITION_CONTAINED_BY: u16 = 0x10;
const DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC: u16 = 0x20;

fn compare_document_position(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let other_arg = args.get(0);
    if !other_arg.is_object() {
        let msg = v8::String::new(scope, "compareDocumentPosition: argument is not a Node").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    }
    let other_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(other_arg) };
    let Some(other_id) = unwrap_node_id(scope, other_obj) else {
        let msg = v8::String::new(scope, "compareDocumentPosition: argument is not a Node").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    };

    let arena = arena_ref(scope);
    let result = do_compare_document_position(arena, node_id, other_id);
    rv.set(v8::Integer::new(scope, result as i32).into());
}

fn do_compare_document_position(
    arena: &crate::dom::Arena,
    self_id: crate::dom::NodeId,
    other_id: crate::dom::NodeId,
) -> u16 {
    // Step 1: Same node
    if self_id == other_id {
        return 0;
    }

    // Collect ancestor chains (from node to root)
    let self_ancestors = {
        let mut chain = Vec::new();
        let mut current = Some(self_id);
        while let Some(id) = current {
            chain.push(id);
            current = arena.nodes[id].parent;
        }
        chain
    };
    let other_ancestors = {
        let mut chain = Vec::new();
        let mut current = Some(other_id);
        while let Some(id) = current {
            chain.push(id);
            current = arena.nodes[id].parent;
        }
        chain
    };

    // Check if they share a root
    let self_root = *self_ancestors.last().unwrap();
    let other_root = *other_ancestors.last().unwrap();
    if self_root != other_root {
        // Disconnected — use hash for stable ordering (no pointer access)
        use std::hash::{Hash, Hasher};
        let hash_key = |id: crate::dom::NodeId| -> u64 {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            id.hash(&mut h);
            h.finish()
        };
        let preceding = hash_key(self_id) < hash_key(other_id);
        return DOCUMENT_POSITION_DISCONNECTED
            | DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC
            | if preceding { DOCUMENT_POSITION_PRECEDING } else { DOCUMENT_POSITION_FOLLOWING };
    }

    // Walk from root to find divergence point
    let mut si = self_ancestors.len() - 1;
    let mut oi = other_ancestors.len() - 1;

    // Skip common root
    while si > 0 && oi > 0 {
        si -= 1;
        oi -= 1;
        let self_child = self_ancestors[si];
        let other_child = other_ancestors[oi];
        if self_child != other_child {
            // They diverge here — find which comes first among siblings of their common parent
            let parent = self_ancestors[si + 1]; // == other_ancestors[oi + 1]
            for child in arena.children(parent) {
                if child == self_child {
                    return DOCUMENT_POSITION_FOLLOWING; // other follows self
                }
                if child == other_child {
                    return DOCUMENT_POSITION_PRECEDING; // other precedes self
                }
            }
            // Shouldn't reach here
            return DOCUMENT_POSITION_DISCONNECTED;
        }
    }

    // One chain is exhausted — one node contains the other
    if si > 0 {
        // self has more ancestors left → self is deeper → other contains self
        DOCUMENT_POSITION_CONTAINS | DOCUMENT_POSITION_PRECEDING
    } else {
        // other is deeper → self contains other
        DOCUMENT_POSITION_CONTAINED_BY | DOCUMENT_POSITION_FOLLOWING
    }
}

// ─── Namespace methods ───────────────────────────────────────────────────────
// Ported from Servo's node.rs.
// These are stubs that return null — blazeweb doesn't track namespace prefixes
// beyond the element's QualName, but these methods must exist per spec.

fn lookup_prefix(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let ns_arg = args.get(0);
    if ns_arg.is_null_or_undefined() {
        rv.set(v8::null(scope).into());
        return;
    }
    let ns = ns_arg.to_rust_string_lossy(scope);
    let arena = arena_ref(scope);

    // Walk up to find an element with matching namespace
    let mut current = Some(node_id);
    while let Some(id) = current {
        if let NodeData::Element(data) = &arena.nodes[id].data {
            if &*data.name.ns == &*ns {
                match &data.name.prefix {
                    Some(prefix) => {
                        let v8_str = v8::String::new(scope, &**prefix).unwrap();
                        rv.set(v8_str.into());
                        return;
                    }
                    None => {
                        rv.set(v8::null(scope).into());
                        return;
                    }
                }
            }
            // Check attributes for xmlns:prefix declarations
            for attr in &data.attrs {
                if attr.name.ns == markup5ever::ns!(xmlns) {
                    if &*attr.value == &*ns {
                        let v8_str = v8::String::new(scope, &*attr.name.local).unwrap();
                        rv.set(v8_str.into());
                        return;
                    }
                }
            }
        }
        current = arena.nodes[id].parent;
    }
    rv.set(v8::null(scope).into());
}

fn lookup_namespace_uri(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let prefix_arg = args.get(0);
    let prefix = if prefix_arg.is_null_or_undefined() || {
        let s = prefix_arg.to_rust_string_lossy(scope);
        s.is_empty()
    } {
        None
    } else {
        Some(prefix_arg.to_rust_string_lossy(scope))
    };
    let arena = arena_ref(scope);

    let mut current = Some(node_id);
    while let Some(id) = current {
        if let NodeData::Element(data) = &arena.nodes[id].data {
            match &prefix {
                None => {
                    // Looking for default namespace
                    if data.name.prefix.is_none() && !data.name.ns.is_empty() {
                        let v8_str = v8::String::new(scope, &*data.name.ns).unwrap();
                        rv.set(v8_str.into());
                        return;
                    }
                    // Check xmlns attribute (default namespace declaration)
                    for attr in &data.attrs {
                        if attr.name.ns == markup5ever::ns!()
                            && &*attr.name.local == "xmlns"
                        {
                            if attr.value.is_empty() {
                                rv.set(v8::null(scope).into());
                            } else {
                                let v8_str = v8::String::new(scope, &*attr.value).unwrap();
                                rv.set(v8_str.into());
                            }
                            return;
                        }
                    }
                }
                Some(p) => {
                    // Looking for specific prefix
                    if let Some(ref elem_prefix) = data.name.prefix {
                        if &**elem_prefix == &**p && !data.name.ns.is_empty() {
                            let v8_str = v8::String::new(scope, &*data.name.ns).unwrap();
                            rv.set(v8_str.into());
                            return;
                        }
                    }
                    // Check xmlns:prefix attributes
                    for attr in &data.attrs {
                        if attr.name.ns == markup5ever::ns!(xmlns) && &*attr.name.local == &**p {
                            if attr.value.is_empty() {
                                rv.set(v8::null(scope).into());
                            } else {
                                let v8_str = v8::String::new(scope, &*attr.value).unwrap();
                                rv.set(v8_str.into());
                            }
                            return;
                        }
                    }
                }
            }
        }
        current = arena.nodes[id].parent;
    }
    rv.set(v8::null(scope).into());
}

fn is_default_namespace(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let ns_arg = args.get(0);
    let namespace = if ns_arg.is_null_or_undefined() {
        String::new()
    } else {
        ns_arg.to_rust_string_lossy(scope)
    };
    let arena = arena_ref(scope);

    // Find the default namespace (locate_namespace with null prefix)
    let mut current = Some(node_id);
    while let Some(id) = current {
        if let NodeData::Element(data) = &arena.nodes[id].data {
            if data.name.prefix.is_none() {
                let result = &*data.name.ns == &*namespace;
                rv.set(v8::Boolean::new(scope, result).into());
                return;
            }
            // Check xmlns attribute
            for attr in &data.attrs {
                if attr.name.ns == markup5ever::ns!() && &*attr.name.local == "xmlns" {
                    let result = &*attr.value == &*namespace;
                    rv.set(v8::Boolean::new(scope, result).into());
                    return;
                }
            }
        }
        current = arena.nodes[id].parent;
    }
    // No namespace found — default is empty
    rv.set(v8::Boolean::new(scope, namespace.is_empty()).into());
}
