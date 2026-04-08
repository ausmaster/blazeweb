/// Advanced document methods — TreeWalker, Range, createEvent, etc.

use crate::dom::node::NodeData;
use crate::js::templates::{arena_mut, arena_ref, unwrap_node_id, wrap_node};

pub(super) fn create_event(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let is_custom = event_type.eq_ignore_ascii_case("customevent");
    let is_error = event_type.eq_ignore_ascii_case("errorevent");
    log::trace!("document.createEvent('{}') is_custom={}", event_type, is_custom);

    // Use build_base_event to create a proper event with all standard properties
    // (preventDefault, stopPropagation, defaultPrevented, target, etc.)
    // We pass empty type "" — the caller will use initEvent to set type/bubbles/cancelable.
    let obj = super::event_constructors::build_base_event(scope, "", &args);

    // initEvent method (legacy DOM Level 2) — overrides type/bubbles/cancelable
    let init = v8::Function::new(scope, |scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue| {
        let this = args.this();
        let type_val = args.get(0).to_rust_string_lossy(scope);
        let bubbles = if args.length() > 1 { args.get(1).boolean_value(scope) } else { false };
        let cancelable = if args.length() > 2 { args.get(2).boolean_value(scope) } else { false };
        let k = v8::String::new(scope, "type").unwrap();
        let v = v8::String::new(scope, &type_val).unwrap();
        this.set(scope, k.into(), v.into());
        let k = v8::String::new(scope, "bubbles").unwrap();
        let v = v8::Boolean::new(scope, bubbles);
        this.set(scope, k.into(), v.into());
        let k = v8::String::new(scope, "cancelable").unwrap();
        let v = v8::Boolean::new(scope, cancelable);
        this.set(scope, k.into(), v.into());
    }).unwrap();
    let k = v8::String::new(scope, "initEvent").unwrap();
    obj.set(scope, k.into(), init.into());

    // For CustomEvent: add initCustomEvent and detail property
    if is_custom {
        let k = v8::String::new(scope, "detail").unwrap();
        let null_val = v8::null(scope);
        obj.set(scope, k.into(), null_val.into());

        let init_custom = v8::Function::new(scope, |scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue| {
            let this = args.this();
            let type_val = args.get(0).to_rust_string_lossy(scope);
            let bubbles = if args.length() > 1 { args.get(1).boolean_value(scope) } else { false };
            let cancelable = if args.length() > 2 { args.get(2).boolean_value(scope) } else { false };
            let detail = if args.length() > 3 { args.get(3) } else { v8::null(scope).into() };

            let k = v8::String::new(scope, "type").unwrap();
            let v = v8::String::new(scope, &type_val).unwrap();
            this.set(scope, k.into(), v.into());
            let k = v8::String::new(scope, "bubbles").unwrap();
            let v = v8::Boolean::new(scope, bubbles);
            this.set(scope, k.into(), v.into());
            let k = v8::String::new(scope, "cancelable").unwrap();
            let v = v8::Boolean::new(scope, cancelable);
            this.set(scope, k.into(), v.into());
            let k = v8::String::new(scope, "detail").unwrap();
            this.set(scope, k.into(), detail);
        }).unwrap();
        let k = v8::String::new(scope, "initCustomEvent").unwrap();
        obj.set(scope, k.into(), init_custom.into());
    }

    // For ErrorEvent: add initErrorEvent method
    if is_error {
        let init_error = v8::Function::new(scope, |scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue| {
            let this = args.this();
            let type_val = args.get(0).to_rust_string_lossy(scope);
            let bubbles = if args.length() > 1 { args.get(1).boolean_value(scope) } else { false };
            let cancelable = if args.length() > 2 { args.get(2).boolean_value(scope) } else { false };
            let k = v8::String::new(scope, "type").unwrap();
            let v = v8::String::new(scope, &type_val).unwrap();
            this.set(scope, k.into(), v.into());
            let k = v8::String::new(scope, "bubbles").unwrap();
            let v = v8::Boolean::new(scope, bubbles);
            this.set(scope, k.into(), v.into());
            let k = v8::String::new(scope, "cancelable").unwrap();
            let v = v8::Boolean::new(scope, cancelable);
            this.set(scope, k.into(), v.into());
            // ErrorEvent-specific: message, filename, lineno
            if args.length() > 3 {
                let k = v8::String::new(scope, "message").unwrap();
                this.set(scope, k.into(), args.get(3));
            }
            if args.length() > 4 {
                let k = v8::String::new(scope, "filename").unwrap();
                this.set(scope, k.into(), args.get(4));
            }
            if args.length() > 5 {
                let k = v8::String::new(scope, "lineno").unwrap();
                this.set(scope, k.into(), args.get(5));
            }
        }).unwrap();
        let k = v8::String::new(scope, "initErrorEvent").unwrap();
        obj.set(scope, k.into(), init_error.into());
        // Default ErrorEvent properties
        for prop in &["message", "filename", "error"] {
            let k = v8::String::new(scope, prop).unwrap();
            let empty = v8::String::new(scope, "").unwrap();
            obj.set(scope, k.into(), empty.into());
        }
        let k = v8::String::new(scope, "lineno").unwrap();
        let zero = v8::Integer::new(scope, 0);
        obj.set(scope, k.into(), zero.into());
        let k = v8::String::new(scope, "colno").unwrap();
        obj.set(scope, k.into(), zero.into());
    }

    rv.set(obj.into());
}

pub(super) fn create_range(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let range = v8::Object::new(scope);
    let noop = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    for name in &["setStart", "setEnd", "setStartBefore", "setStartAfter",
                   "setEndBefore", "setEndAfter", "collapse", "selectNode",
                   "selectNodeContents", "deleteContents", "insertNode",
                   "surroundContents", "detach"] {
        let k = v8::String::new(scope, name).unwrap();
        range.set(scope, k.into(), noop.into());
    }
    let k = v8::String::new(scope, "collapsed").unwrap();
    let v = v8::Boolean::new(scope, true);
    range.set(scope, k.into(), v.into());
    let zero = v8::Integer::new(scope, 0);
    for name in &["startOffset", "endOffset"] {
        let k = v8::String::new(scope, name).unwrap();
        range.set(scope, k.into(), zero.into());
    }
    let null = v8::null(scope);
    for name in &["startContainer", "endContainer", "commonAncestorContainer"] {
        let k = v8::String::new(scope, name).unwrap();
        range.set(scope, k.into(), null.into());
    }

    // createContextualFragment — uses fragment parser
    let ccf = v8::Function::new(scope, create_contextual_fragment).unwrap();
    let k = v8::String::new(scope, "createContextualFragment").unwrap();
    range.set(scope, k.into(), ccf.into());

    // cloneRange
    let clone_range = v8::Function::new(scope, create_range).unwrap();
    let k = v8::String::new(scope, "cloneRange").unwrap();
    range.set(scope, k.into(), clone_range.into());

    // getBoundingClientRect
    let gbcr = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let obj = v8::Object::new(scope);
        let zero = v8::Number::new(scope, 0.0);
        for name in &["top", "left", "right", "bottom", "width", "height", "x", "y"] {
            let k = v8::String::new(scope, name).unwrap();
            obj.set(scope, k.into(), zero.into());
        }
        rv.set(obj.into());
    }).unwrap();
    let k = v8::String::new(scope, "getBoundingClientRect").unwrap();
    range.set(scope, k.into(), gbcr.into());

    // getClientRects
    let gcr = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        rv.set(v8::Array::new(scope, 0).into());
    }).unwrap();
    let k = v8::String::new(scope, "getClientRects").unwrap();
    range.set(scope, k.into(), gcr.into());

    // cloneContents
    let clone_contents = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let arena = crate::js::templates::arena_mut(scope);
        let frag = arena.new_node(NodeData::Document);
        rv.set(wrap_node(scope, frag).into());
    }).unwrap();
    let k = v8::String::new(scope, "cloneContents").unwrap();
    range.set(scope, k.into(), clone_contents.into());

    // extractContents
    let k = v8::String::new(scope, "extractContents").unwrap();
    range.set(scope, k.into(), clone_contents.into());

    // toString
    let to_str = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        rv.set(v8::String::new(scope, "").unwrap().into());
    }).unwrap();
    let k = v8::String::new(scope, "toString").unwrap();
    range.set(scope, k.into(), to_str.into());

    rv.set(range.into());
}

pub(super) fn create_contextual_fragment(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let html = args.get(0).to_rust_string_lossy(scope);
    let arena = crate::js::templates::arena_mut(scope);
    let frag_id = arena.new_node(NodeData::Document);

    if !html.is_empty() {
        let fragment_arena = crate::dom::treesink::parse_fragment(&html, "body", true);
        if let Some(html_wrapper) = fragment_arena.children(fragment_arena.document).next() {
            for child in fragment_arena.children(html_wrapper) {
                let new_id = super::element::clone_across_arenas(arena, &fragment_arena, child);
                arena.append_child(frag_id, new_id);
            }
        }
    }

    rv.set(wrap_node(scope, frag_id).into());
}

// ---------------------------------------------------------------------------
// TreeWalker — Full WHATWG DOM §6.2 implementation
// Ported from Servo treewalker.rs
// ---------------------------------------------------------------------------

use crate::dom::NodeId;
use crate::dom::Arena;

const FILTER_ACCEPT: u16 = 1;
const FILTER_REJECT: u16 = 2;
const FILTER_SKIP: u16 = 3;

/// Get the root NodeId stored in the walker's private field.
fn tw_root_id(scope: &mut v8::HandleScope, walker: v8::Local<v8::Object>) -> Option<NodeId> {
    let pk = v8::String::new(scope, "__rootId").unwrap();
    let hidden_key = v8::Private::for_api(scope, Some(pk));
    walker.get_private(scope, hidden_key)
        .and_then(|v| v8::Local::<v8::External>::try_from(v).ok())
        .map(|ext| unsafe { *(ext.value() as *const NodeId) })
}

/// Get the currentNode's NodeId from the walker.
fn tw_current_id(scope: &mut v8::HandleScope, walker: v8::Local<v8::Object>) -> Option<NodeId> {
    let k = v8::String::new(scope, "currentNode").unwrap();
    let val = walker.get(scope, k.into())?;
    if !val.is_object() { return None; }
    let obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(val) };
    unwrap_node_id(scope, obj)
}

/// Set currentNode on the walker and return the wrapped node.
fn tw_set_current<'s>(scope: &mut v8::HandleScope<'s>, walker: v8::Local<v8::Object>, node_id: NodeId) -> v8::Local<'s, v8::Object> {
    let wrapped = wrap_node(scope, node_id);
    let k = v8::String::new(scope, "currentNode").unwrap();
    walker.set(scope, k.into(), wrapped.into());
    wrapped
}

/// Get whatToShow from the walker.
fn tw_what_to_show(scope: &mut v8::HandleScope, walker: v8::Local<v8::Object>) -> u32 {
    let k = v8::String::new(scope, "whatToShow").unwrap();
    walker.get(scope, k.into())
        .and_then(|v| v.uint32_value(scope))
        .unwrap_or(0xFFFFFFFF)
}

/// accept_node — WHATWG DOM §6 "Filtering" algorithm.
/// Returns FILTER_ACCEPT, FILTER_REJECT, or FILTER_SKIP.
/// Returns None if a JS exception was thrown.
fn tw_accept_node(
    scope: &mut v8::HandleScope,
    walker: v8::Local<v8::Object>,
    node_id: NodeId,
    what_to_show: u32,
) -> Option<u16> {
    // Step 1: Check active flag (re-entrancy guard)
    let active_pk = v8::String::new(scope, "__active").unwrap();
    let active_key = v8::Private::for_api(scope, Some(active_pk));
    let is_active = walker.get_private(scope, active_key)
        .map(|v| v.boolean_value(scope))
        .unwrap_or(false);
    if is_active {
        let msg = v8::String::new(scope, "InvalidStateError: TreeWalker filter is active").unwrap();
        let exc = v8::Exception::error(scope, msg);
        scope.throw_exception(exc);
        return None;
    }

    // Step 2-3: Check whatToShow bitmask
    let arena = arena_ref(scope);
    let node_type = match &arena.nodes[node_id].data {
        NodeData::Element(_) => 1,
        NodeData::Text(_) => 3,
        NodeData::Comment(_) => 8,
        NodeData::Document => 9,
        NodeData::DocumentFragment => 11,
        NodeData::Doctype { .. } => 10,
    };
    let n = node_type - 1;
    if what_to_show != 0xFFFFFFFF && (what_to_show & (1 << n)) == 0 {
        return Some(FILTER_SKIP);
    }

    // Step 4-8: Call filter if present
    let filter_pk = v8::String::new(scope, "__filter").unwrap();
    let filter_key = v8::Private::for_api(scope, Some(filter_pk));
    let filter_val = walker.get_private(scope, filter_key);
    let has_filter = filter_val.as_ref().map(|v| !v.is_undefined() && !v.is_null()).unwrap_or(false);

    if !has_filter {
        return Some(FILTER_ACCEPT);
    }
    let filter_val = filter_val.unwrap();

    // Set active = true
    let true_val = v8::Boolean::new(scope, true);
    walker.set_private(scope, active_key, true_val.into());

    let node_wrapped = wrap_node(scope, node_id);
    let result;

    if filter_val.is_function() {
        // Filter is a function
        let func = unsafe { v8::Local::<v8::Function>::cast_unchecked(filter_val) };
        let undefined = v8::undefined(scope);
        let try_catch = &mut v8::TryCatch::new(scope);
        let ret = func.call(try_catch, undefined.into(), &[node_wrapped.into()]);
        if try_catch.has_caught() {
            // Re-set active = false, propagate exception
            let active_pk = v8::String::new(try_catch, "__active").unwrap();
            let active_key = v8::Private::for_api(try_catch, Some(active_pk));
            let false_val = v8::Boolean::new(try_catch, false);
            walker.set_private(try_catch, active_key, false_val.into());
            // Exception propagates
            return None;
        }
        result = ret.and_then(|v| v.uint32_value(try_catch)).unwrap_or(FILTER_ACCEPT as u32) as u16;
        // Reset active
        let active_pk = v8::String::new(try_catch, "__active").unwrap();
        let active_key = v8::Private::for_api(try_catch, Some(active_pk));
        let false_val = v8::Boolean::new(try_catch, false);
        walker.set_private(try_catch, active_key, false_val.into());
    } else if filter_val.is_object() {
        // Filter is an object with acceptNode method
        let filter_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(filter_val) };
        let method_key = v8::String::new(scope, "acceptNode").unwrap();
        let method_val = filter_obj.get(scope, method_key.into());
        if let Some(method) = method_val {
            if method.is_function() {
                let func = unsafe { v8::Local::<v8::Function>::cast_unchecked(method) };
                let try_catch = &mut v8::TryCatch::new(scope);
                let ret = func.call(try_catch, filter_val, &[node_wrapped.into()]);
                if try_catch.has_caught() {
                    let active_pk = v8::String::new(try_catch, "__active").unwrap();
                    let active_key = v8::Private::for_api(try_catch, Some(active_pk));
                    let false_val = v8::Boolean::new(try_catch, false);
                    walker.set_private(try_catch, active_key, false_val.into());
                    return None;
                }
                result = ret.and_then(|v| v.uint32_value(try_catch)).unwrap_or(FILTER_ACCEPT as u32) as u16;
                let active_pk = v8::String::new(try_catch, "__active").unwrap();
                let active_key = v8::Private::for_api(try_catch, Some(active_pk));
                let false_val = v8::Boolean::new(try_catch, false);
                walker.set_private(try_catch, active_key, false_val.into());
            } else {
                let false_val = v8::Boolean::new(scope, false);
                walker.set_private(scope, active_key, false_val.into());
                result = FILTER_ACCEPT;
            }
        } else {
            let false_val = v8::Boolean::new(scope, false);
            walker.set_private(scope, active_key, false_val.into());
            result = FILTER_ACCEPT;
        }
    } else {
        let false_val = v8::Boolean::new(scope, false);
        walker.set_private(scope, active_key, false_val.into());
        result = FILTER_ACCEPT;
    }

    Some(result)
}

/// First following node not following root — for nextNode().
fn first_following_not_following_root(arena: &Arena, node: NodeId, root: NodeId) -> Option<NodeId> {
    if let Some(ns) = arena.nodes[node].next_sibling {
        return Some(ns);
    }
    let mut candidate = node;
    loop {
        if candidate == root { return None; }
        match arena.nodes[candidate].parent {
            None => return None,
            Some(parent) => {
                if parent == root { return None; }
                if let Some(ns) = arena.nodes[parent].next_sibling {
                    return Some(ns);
                }
                candidate = parent;
            }
        }
    }
}


pub(super) fn create_tree_walker(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let root_arg = args.get(0);
    let what_to_show = if args.length() > 1 && !args.get(1).is_undefined() {
        args.get(1).uint32_value(scope).unwrap_or(0xFFFFFFFF)
    } else {
        0xFFFFFFFF
    };

    let root_obj = if root_arg.is_object() {
        unsafe { v8::Local::<v8::Object>::cast_unchecked(root_arg) }
    } else {
        rv.set(v8::null(scope).into());
        return;
    };

    let walker = v8::Object::new(scope);

    // Visible properties
    let k = v8::String::new(scope, "root").unwrap();
    walker.set(scope, k.into(), root_arg);
    let k = v8::String::new(scope, "whatToShow").unwrap();
    let v = v8::Integer::new(scope, what_to_show as i32);
    walker.set(scope, k.into(), v.into());
    let k = v8::String::new(scope, "currentNode").unwrap();
    walker.set(scope, k.into(), root_arg);

    // Private: root NodeId
    if let Some(root_id) = unwrap_node_id(scope, root_obj) {
        let boxed = Box::new(root_id);
        let external = v8::External::new(scope, Box::into_raw(boxed) as *mut std::ffi::c_void);
        let pk = v8::String::new(scope, "__rootId").unwrap();
        let hidden_key = v8::Private::for_api(scope, Some(pk));
        walker.set_private(scope, hidden_key, external.into());
    }

    // Private: filter (3rd argument)
    let filter_pk = v8::String::new(scope, "__filter").unwrap();
    let filter_key = v8::Private::for_api(scope, Some(filter_pk));
    if args.length() > 2 && !args.get(2).is_null() && !args.get(2).is_undefined() {
        walker.set_private(scope, filter_key, args.get(2));
        // Expose filter property
        let k = v8::String::new(scope, "filter").unwrap();
        walker.set(scope, k.into(), args.get(2));
    } else {
        let null_val = v8::null(scope);
        walker.set_private(scope, filter_key, null_val.into());
        let k = v8::String::new(scope, "filter").unwrap();
        walker.set(scope, k.into(), null_val.into());
    }

    // Private: active flag
    let active_pk = v8::String::new(scope, "__active").unwrap();
    let active_key = v8::Private::for_api(scope, Some(active_pk));
    let false_val = v8::Boolean::new(scope, false);
    walker.set_private(scope, active_key, false_val.into());

    // Install all 7 methods
    macro_rules! set_method {
        ($scope:expr, $obj:expr, $name:expr, $cb:expr) => {{
            let func = v8::Function::new($scope, $cb).unwrap();
            let k = v8::String::new($scope, $name).unwrap();
            $obj.set($scope, k.into(), func.into());
        }};
    }
    set_method!(scope, walker, "parentNode", tw_parent_node);
    set_method!(scope, walker, "firstChild", tw_first_child);
    set_method!(scope, walker, "lastChild", tw_last_child);
    set_method!(scope, walker, "previousSibling", tw_previous_sibling);
    set_method!(scope, walker, "nextSibling", tw_next_sibling);
    set_method!(scope, walker, "previousNode", tw_previous_node);
    set_method!(scope, walker, "nextNode", tw_next_node);

    rv.set(walker.into());
}

/// TreeWalker.parentNode()
fn tw_parent_node(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let walker = args.this();
    let Some(root_id) = tw_root_id(scope, walker) else { rv.set(v8::null(scope).into()); return; };
    let Some(mut node) = tw_current_id(scope, walker) else { rv.set(v8::null(scope).into()); return; };
    let what_to_show = tw_what_to_show(scope, walker);

    while node != root_id {
        let arena = arena_ref(scope);
        let parent = arena.nodes[node].parent;
        match parent {
            Some(p) => {
                node = p;
                let result = tw_accept_node(scope, walker, node, what_to_show);
                match result {
                    Some(FILTER_ACCEPT) => {
                        let wrapped = tw_set_current(scope, walker, node);
                        rv.set(wrapped.into());
                        return;
                    }
                    None => return, // exception thrown
                    _ => {} // SKIP or REJECT: continue walking up
                }
            }
            None => break,
        }
    }
    rv.set(v8::null(scope).into());
}

/// TreeWalker.firstChild()
fn tw_first_child(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let walker = args.this();
    tw_traverse_children(scope, walker, &mut rv, true);
}

/// TreeWalker.lastChild()
fn tw_last_child(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let walker = args.this();
    tw_traverse_children(scope, walker, &mut rv, false);
}

/// Generic traverse_children — `first` = true for firstChild, false for lastChild.
fn tw_traverse_children(
    scope: &mut v8::HandleScope,
    walker: v8::Local<v8::Object>,
    rv: &mut v8::ReturnValue,
    first: bool,
) {
    let Some(root_id) = tw_root_id(scope, walker) else { rv.set(v8::null(scope).into()); return; };
    let Some(current) = tw_current_id(scope, walker) else { rv.set(v8::null(scope).into()); return; };
    let what_to_show = tw_what_to_show(scope, walker);

    let arena = arena_ref(scope);
    let child = if first { arena.nodes[current].first_child } else { arena.nodes[current].last_child };
    let Some(mut node) = child else { rv.set(v8::null(scope).into()); return; };

    'main: loop {
        let result = tw_accept_node(scope, walker, node, what_to_show);
        match result {
            Some(FILTER_ACCEPT) => {
                let wrapped = tw_set_current(scope, walker, node);
                rv.set(wrapped.into());
                return;
            }
            Some(FILTER_SKIP) => {
                let arena = arena_ref(scope);
                let child = if first { arena.nodes[node].first_child } else { arena.nodes[node].last_child };
                if let Some(c) = child {
                    node = c;
                    continue 'main;
                }
            }
            None => return, // exception
            _ => {} // FILTER_REJECT: fall through to sibling walk
        }

        // Walk to sibling or up to parent
        loop {
            let arena = arena_ref(scope);
            let sibling = if first { arena.nodes[node].next_sibling } else { arena.nodes[node].prev_sibling };
            if let Some(s) = sibling {
                node = s;
                continue 'main;
            }
            let arena = arena_ref(scope);
            let parent = arena.nodes[node].parent;
            match parent {
                None => { rv.set(v8::null(scope).into()); return; }
                Some(p) if p == root_id || p == current => { rv.set(v8::null(scope).into()); return; }
                Some(p) => { node = p; }
            }
        }
    }
}

/// TreeWalker.nextSibling()
fn tw_next_sibling(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let walker = args.this();
    tw_traverse_siblings(scope, walker, &mut rv, true);
}

/// TreeWalker.previousSibling()
fn tw_previous_sibling(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let walker = args.this();
    tw_traverse_siblings(scope, walker, &mut rv, false);
}

/// Generic traverse_siblings — `next` = true for nextSibling, false for previousSibling.
fn tw_traverse_siblings(
    scope: &mut v8::HandleScope,
    walker: v8::Local<v8::Object>,
    rv: &mut v8::ReturnValue,
    next: bool,
) {
    let Some(root_id) = tw_root_id(scope, walker) else { rv.set(v8::null(scope).into()); return; };
    let Some(current) = tw_current_id(scope, walker) else { rv.set(v8::null(scope).into()); return; };
    let what_to_show = tw_what_to_show(scope, walker);

    let mut node = current;
    if node == root_id { rv.set(v8::null(scope).into()); return; }

    loop {
        let arena = arena_ref(scope);
        let mut sibling_opt = if next { arena.nodes[node].next_sibling } else { arena.nodes[node].prev_sibling };

        while let Some(sibling) = sibling_opt {
            node = sibling;
            let result = tw_accept_node(scope, walker, node, what_to_show);
            match result {
                Some(FILTER_ACCEPT) => {
                    let wrapped = tw_set_current(scope, walker, node);
                    rv.set(wrapped.into());
                    return;
                }
                None => return, // exception
                _ => {}
            }

            // Try to descend into child (first child for next, last child for previous)
            let arena = arena_ref(scope);
            let child = if next { arena.nodes[node].first_child } else { arena.nodes[node].last_child };

            match (result, child) {
                (Some(FILTER_REJECT), _) | (_, None) => {
                    let arena = arena_ref(scope);
                    sibling_opt = if next { arena.nodes[node].next_sibling } else { arena.nodes[node].prev_sibling };
                }
                (_, Some(c)) => {
                    sibling_opt = Some(c);
                }
            }
        }

        // Walk up to parent
        let arena = arena_ref(scope);
        match arena.nodes[node].parent {
            None => { rv.set(v8::null(scope).into()); return; }
            Some(p) if p == root_id => { rv.set(v8::null(scope).into()); return; }
            Some(p) => {
                node = p;
                let result = tw_accept_node(scope, walker, node, what_to_show);
                match result {
                    Some(FILTER_ACCEPT) => { rv.set(v8::null(scope).into()); return; }
                    None => return,
                    _ => {} // continue loop
                }
            }
        }
    }
}

/// TreeWalker.previousNode()
fn tw_previous_node(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let walker = args.this();
    let Some(root_id) = tw_root_id(scope, walker) else { rv.set(v8::null(scope).into()); return; };
    let Some(current) = tw_current_id(scope, walker) else { rv.set(v8::null(scope).into()); return; };
    let what_to_show = tw_what_to_show(scope, walker);

    let mut node = current;
    while node != root_id {
        let arena = arena_ref(scope);
        let mut sibling_opt = arena.nodes[node].prev_sibling;

        while let Some(sibling) = sibling_opt {
            node = sibling;
            loop {
                let result = tw_accept_node(scope, walker, node, what_to_show);
                match result {
                    Some(FILTER_REJECT) => break,
                    None => return,
                    _ => {
                        // If node has a child, descend to last child
                        let arena = arena_ref(scope);
                        if arena.nodes[node].first_child.is_some() {
                            node = arena.nodes[node].last_child.unwrap();
                            continue;
                        }
                        if result == Some(FILTER_ACCEPT) {
                            let wrapped = tw_set_current(scope, walker, node);
                            rv.set(wrapped.into());
                            return;
                        }
                        break;
                    }
                }
            }
            let arena = arena_ref(scope);
            sibling_opt = arena.nodes[node].prev_sibling;
        }

        // Go to parent
        if node == root_id {
            rv.set(v8::null(scope).into());
            return;
        }
        let arena = arena_ref(scope);
        match arena.nodes[node].parent {
            None => { rv.set(v8::null(scope).into()); return; }
            Some(p) => {
                node = p;
                let result = tw_accept_node(scope, walker, node, what_to_show);
                match result {
                    Some(FILTER_ACCEPT) => {
                        let wrapped = tw_set_current(scope, walker, node);
                        rv.set(wrapped.into());
                        return;
                    }
                    None => return,
                    _ => {} // continue
                }
            }
        }
    }
    rv.set(v8::null(scope).into());
}

/// TreeWalker.nextNode()
fn tw_next_node(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let walker = args.this();
    let Some(root_id) = tw_root_id(scope, walker) else { rv.set(v8::null(scope).into()); return; };
    let Some(current) = tw_current_id(scope, walker) else { rv.set(v8::null(scope).into()); return; };
    let what_to_show = tw_what_to_show(scope, walker);

    let mut node = current;
    let mut result: u16 = FILTER_ACCEPT;

    loop {
        // While result is not REJECT and node has a child, descend
        while result != FILTER_REJECT {
            let arena = arena_ref(scope);
            let child = arena.nodes[node].first_child;
            match child {
                None => break,
                Some(c) => {
                    node = c;
                    match tw_accept_node(scope, walker, node, what_to_show) {
                        Some(r) => {
                            result = r;
                            if result == FILTER_ACCEPT {
                                let wrapped = tw_set_current(scope, walker, node);
                                rv.set(wrapped.into());
                                return;
                            }
                        }
                        None => return, // exception
                    }
                }
            }
        }

        // Find first following node not following root
        let arena = arena_ref(scope);
        match first_following_not_following_root(arena, node, root_id) {
            None => { rv.set(v8::null(scope).into()); return; }
            Some(following) => {
                node = following;
                match tw_accept_node(scope, walker, node, what_to_show) {
                    Some(r) => {
                        result = r;
                        if result == FILTER_ACCEPT {
                            let wrapped = tw_set_current(scope, walker, node);
                            rv.set(wrapped.into());
                            return;
                        }
                    }
                    None => return,
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// NodeIterator — WHATWG DOM §6.1
// Separate from TreeWalker: flat iteration, REJECT == SKIP, referenceNode state.
// Ported from Servo nodeiterator.rs.
// ---------------------------------------------------------------------------

/// Depth-first previous node (reverse document order), bounded by root.
/// The root is part of the iterator collection, so parent==root IS reachable.
fn depth_first_prev(arena: &Arena, node: NodeId, root: NodeId) -> Option<NodeId> {
    if node == root { return None; } // Can't go before root
    // Try previous sibling's deepest last descendant
    if let Some(ps) = arena.nodes[node].prev_sibling {
        let mut n = ps;
        while let Some(lc) = arena.nodes[n].last_child {
            n = lc;
        }
        return Some(n);
    }
    // Parent — including root itself (root is in the iterator collection)
    arena.nodes[node].parent
}

/// Depth-first next node bounded by root.
fn depth_first_next_bounded(arena: &Arena, node: NodeId, root: NodeId) -> Option<NodeId> {
    if let Some(fc) = arena.nodes[node].first_child {
        return Some(fc);
    }
    let mut current = node;
    loop {
        if current == root { return None; }
        if let Some(ns) = arena.nodes[current].next_sibling {
            return Some(ns);
        }
        match arena.nodes[current].parent {
            None => return None,
            Some(p) => {
                if p == root { return None; }
                current = p;
            }
        }
    }
}

pub(super) fn create_node_iterator(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let root_arg = args.get(0);
    let what_to_show = if args.length() > 1 && !args.get(1).is_undefined() {
        args.get(1).uint32_value(scope).unwrap_or(0xFFFFFFFF)
    } else {
        0xFFFFFFFF
    };

    let root_obj = if root_arg.is_object() {
        unsafe { v8::Local::<v8::Object>::cast_unchecked(root_arg) }
    } else {
        rv.set(v8::null(scope).into());
        return;
    };

    let ni = v8::Object::new(scope);

    // Visible properties
    let k = v8::String::new(scope, "root").unwrap();
    ni.set(scope, k.into(), root_arg);
    let k = v8::String::new(scope, "whatToShow").unwrap();
    let v = v8::Number::new(scope, what_to_show as f64);
    ni.set(scope, k.into(), v.into());
    let k = v8::String::new(scope, "referenceNode").unwrap();
    ni.set(scope, k.into(), root_arg);
    let k = v8::String::new(scope, "pointerBeforeReferenceNode").unwrap();
    let v = v8::Boolean::new(scope, true);
    ni.set(scope, k.into(), v.into());

    // Private: root NodeId
    if let Some(root_id) = unwrap_node_id(scope, root_obj) {
        let boxed = Box::new(root_id);
        let external = v8::External::new(scope, Box::into_raw(boxed) as *mut std::ffi::c_void);
        let pk = v8::String::new(scope, "__rootId").unwrap();
        let hidden_key = v8::Private::for_api(scope, Some(pk));
        ni.set_private(scope, hidden_key, external.into());
    }

    // Private: filter (3rd argument) — reuse same pattern as TreeWalker
    let filter_pk = v8::String::new(scope, "__filter").unwrap();
    let filter_key = v8::Private::for_api(scope, Some(filter_pk));
    if args.length() > 2 && !args.get(2).is_null() && !args.get(2).is_undefined() {
        ni.set_private(scope, filter_key, args.get(2));
        let k = v8::String::new(scope, "filter").unwrap();
        ni.set(scope, k.into(), args.get(2));
    } else {
        let null_val = v8::null(scope);
        ni.set_private(scope, filter_key, null_val.into());
        let k = v8::String::new(scope, "filter").unwrap();
        ni.set(scope, k.into(), null_val.into());
    }

    // Private: active flag
    let active_pk = v8::String::new(scope, "__active").unwrap();
    let active_key = v8::Private::for_api(scope, Some(active_pk));
    let false_val = v8::Boolean::new(scope, false);
    ni.set_private(scope, active_key, false_val.into());

    // Methods
    macro_rules! set_method {
        ($scope:expr, $obj:expr, $name:expr, $cb:expr) => {{
            let func = v8::Function::new($scope, $cb).unwrap();
            let k = v8::String::new($scope, $name).unwrap();
            $obj.set($scope, k.into(), func.into());
        }};
    }
    set_method!(scope, ni, "nextNode", ni_next_node);
    set_method!(scope, ni, "previousNode", ni_previous_node);
    set_method!(scope, ni, "detach", ni_detach);

    rv.set(ni.into());
}

/// Get the referenceNode's NodeId from the iterator.
fn ni_reference_id(scope: &mut v8::HandleScope, ni: v8::Local<v8::Object>) -> Option<NodeId> {
    let k = v8::String::new(scope, "referenceNode").unwrap();
    let val = ni.get(scope, k.into())?;
    if !val.is_object() { return None; }
    let obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(val) };
    unwrap_node_id(scope, obj)
}

/// Get pointerBeforeReferenceNode.
fn ni_pointer_before(scope: &mut v8::HandleScope, ni: v8::Local<v8::Object>) -> bool {
    let k = v8::String::new(scope, "pointerBeforeReferenceNode").unwrap();
    ni.get(scope, k.into())
        .map(|v| v.boolean_value(scope))
        .unwrap_or(true)
}

/// Set referenceNode and pointerBeforeReferenceNode.
fn ni_set_reference(scope: &mut v8::HandleScope, ni: v8::Local<v8::Object>, node_id: NodeId, before: bool) {
    let wrapped = wrap_node(scope, node_id);
    let k = v8::String::new(scope, "referenceNode").unwrap();
    ni.set(scope, k.into(), wrapped.into());
    let k = v8::String::new(scope, "pointerBeforeReferenceNode").unwrap();
    let v = v8::Boolean::new(scope, before);
    ni.set(scope, k.into(), v.into());
}

/// accept_node for NodeIterator — same filtering as TreeWalker.
/// tw_accept_node is reused (it works on any object with __active, __filter privates).
fn ni_accept_node(
    scope: &mut v8::HandleScope,
    ni: v8::Local<v8::Object>,
    node_id: NodeId,
    what_to_show: u32,
) -> Option<u16> {
    // Reuse TreeWalker's accept_node — same private key layout
    tw_accept_node(scope, ni, node_id, what_to_show)
}

/// NodeIterator.nextNode() — WHATWG DOM §6.1
fn ni_next_node(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let ni = args.this();
    let Some(root_id) = tw_root_id(scope, ni) else { rv.set(v8::null(scope).into()); return; };
    let Some(ref_node) = ni_reference_id(scope, ni) else { rv.set(v8::null(scope).into()); return; };
    let pointer_before = ni_pointer_before(scope, ni);
    let what_to_show = tw_what_to_show(scope, ni);

    // Per spec: if pointer is before reference node, try accepting reference node first
    if pointer_before {
        // Move pointer to after
        let k = v8::String::new(scope, "pointerBeforeReferenceNode").unwrap();
        let v = v8::Boolean::new(scope, false);
        ni.set(scope, k.into(), v.into());

        let result = ni_accept_node(scope, ni, ref_node, what_to_show);
        match result {
            Some(FILTER_ACCEPT) => {
                // referenceNode is already ref_node, pointer is now after
                let wrapped = wrap_node(scope, ref_node);
                rv.set(wrapped.into());
                return;
            }
            None => return, // exception
            _ => {} // SKIP or REJECT (both act the same in NodeIterator)
        }
    }

    // Walk forward in document order from ref_node, bounded by root
    let mut node = ref_node;
    loop {
        let arena = arena_ref(scope);
        let next = depth_first_next_bounded(arena, node, root_id);
        match next {
            None => { rv.set(v8::null(scope).into()); return; }
            Some(n) => {
                node = n;
                let result = ni_accept_node(scope, ni, node, what_to_show);
                match result {
                    Some(FILTER_ACCEPT) => {
                        ni_set_reference(scope, ni, node, false);
                        let wrapped = wrap_node(scope, node);
                        rv.set(wrapped.into());
                        return;
                    }
                    None => return, // exception
                    _ => {} // SKIP/REJECT: continue (both identical in NodeIterator)
                }
            }
        }
    }
}

/// NodeIterator.previousNode() — WHATWG DOM §6.1
fn ni_previous_node(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let ni = args.this();
    let Some(root_id) = tw_root_id(scope, ni) else { rv.set(v8::null(scope).into()); return; };
    let Some(ref_node) = ni_reference_id(scope, ni) else { rv.set(v8::null(scope).into()); return; };
    let pointer_before = ni_pointer_before(scope, ni);
    let what_to_show = tw_what_to_show(scope, ni);

    // Per spec: if pointer is after reference node, try accepting reference node first
    if !pointer_before {
        // Move pointer to before
        let k = v8::String::new(scope, "pointerBeforeReferenceNode").unwrap();
        let v = v8::Boolean::new(scope, true);
        ni.set(scope, k.into(), v.into());

        let result = ni_accept_node(scope, ni, ref_node, what_to_show);
        match result {
            Some(FILTER_ACCEPT) => {
                let wrapped = wrap_node(scope, ref_node);
                rv.set(wrapped.into());
                return;
            }
            None => return,
            _ => {}
        }
    }

    // Walk backward in reverse document order from ref_node, bounded by root
    let mut node = ref_node;
    loop {
        if node == root_id { rv.set(v8::null(scope).into()); return; }
        let arena = arena_ref(scope);
        let prev = depth_first_prev(arena, node, root_id);
        match prev {
            None => { rv.set(v8::null(scope).into()); return; }
            Some(n) => {
                node = n;
                let result = ni_accept_node(scope, ni, node, what_to_show);
                match result {
                    Some(FILTER_ACCEPT) => {
                        ni_set_reference(scope, ni, node, true);
                        let wrapped = wrap_node(scope, node);
                        rv.set(wrapped.into());
                        return;
                    }
                    None => return,
                    _ => {} // SKIP/REJECT both skip in NodeIterator
                }
            }
        }
    }
}

/// NodeIterator.detach() — no-op per spec.
fn ni_detach(
    _scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {}

pub(super) fn element_from_point(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    // Return document.body
    let arena = arena_ref(scope);
    if let Some(html) = super::document::find_document_element(arena) {
        if let Some(body) = super::document::find_child_element(arena, html, "body") {
            rv.set(wrap_node(scope, body).into());
            return;
        }
    }
    rv.set(v8::null(scope).into());
}

pub(super) fn elements_from_point(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let arr = v8::Array::new(scope, 0);
    let arena = arena_ref(scope);
    if let Some(html) = super::document::find_document_element(arena) {
        if let Some(body) = super::document::find_child_element(arena, html, "body") {
            let wrapped = wrap_node(scope, body);
            arr.set_index(scope, 0, wrapped.into());
        }
    }
    rv.set(arr.into());
}

pub(super) fn document_get_selection(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    // Same as window.getSelection()
    let obj = v8::Object::new(scope);
    let k = v8::String::new(scope, "rangeCount").unwrap();
    let v = v8::Integer::new(scope, 0);
    obj.set(scope, k.into(), v.into());
    let k = v8::String::new(scope, "isCollapsed").unwrap();
    let v = v8::Boolean::new(scope, true);
    obj.set(scope, k.into(), v.into());
    let to_str = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        rv.set(v8::String::new(scope, "").unwrap().into());
    }).unwrap();
    let k = v8::String::new(scope, "toString").unwrap();
    obj.set(scope, k.into(), to_str.into());
    let noop = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    for name in &["removeAllRanges", "addRange"] {
        let k = v8::String::new(scope, name).unwrap();
        obj.set(scope, k.into(), noop.into());
    }
    rv.set(obj.into());
}

pub(super) fn document_noop(
    _scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
}

pub(super) fn document_exec_command(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    rv.set(v8::Boolean::new(scope, false).into());
}

pub(super) fn adopt_node(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let node_arg = args.get(0);
    if !node_arg.is_object() {
        let msg = v8::String::new(scope, "Failed to execute 'adoptNode': parameter 1 is not a Node").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    }
    let obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(node_arg) };
    let Some(node_id) = unwrap_node_id(scope, obj) else {
        let msg = v8::String::new(scope, "Failed to execute 'adoptNode': parameter 1 is not a Node").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    };

    let arena = arena_mut(scope);
    // Document nodes cannot be adopted (NotSupportedError)
    if matches!(&arena.nodes[node_id].data, NodeData::Document) {
        let msg = v8::String::new(scope, "Failed to execute 'adoptNode' on 'Document': The node provided is a document, which may not be adopted.").unwrap();
        let exc = v8::Exception::error(scope, msg);
        if let Some(exc_obj) = exc.to_object(scope) {
            let name_key = v8::String::new(scope, "name").unwrap();
            let name_val = v8::String::new(scope, "NotSupportedError").unwrap();
            exc_obj.set(scope, name_key.into(), name_val.into());
        }
        scope.throw_exception(exc);
        return;
    }
    // Remove from current parent if any
    if arena.nodes[node_id].parent.is_some() {
        arena.detach(node_id);
        // Update connectivity flags
        arena.set_connected_recursive(node_id, false);
    }
    rv.set(node_arg);
}

pub(super) fn import_node(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let node_arg = args.get(0);
    if !node_arg.is_object() {
        let msg = v8::String::new(scope, "Failed to execute 'importNode': parameter 1 is not a Node").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    }
    let obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(node_arg) };
    let Some(node_id) = unwrap_node_id(scope, obj) else {
        let msg = v8::String::new(scope, "Failed to execute 'importNode': parameter 1 is not a Node").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    };
    let deep = if args.length() > 1 { args.get(1).boolean_value(scope) } else { false };

    let arena = arena_mut(scope);
    // Document nodes cannot be imported (NotSupportedError)
    if matches!(&arena.nodes[node_id].data, NodeData::Document) {
        let msg = v8::String::new(scope, "Failed to execute 'importNode' on 'Document': The node provided is a document, which may not be imported.").unwrap();
        let exc = v8::Exception::error(scope, msg);
        if let Some(exc_obj) = exc.to_object(scope) {
            let name_key = v8::String::new(scope, "name").unwrap();
            let name_val = v8::String::new(scope, "NotSupportedError").unwrap();
            exc_obj.set(scope, name_key.into(), name_val.into());
        }
        scope.throw_exception(exc);
        return;
    }

    let clone_id = if deep {
        arena.deep_clone(node_id)
    } else {
        // Shallow clone — just the node data, no children
        let data = arena.nodes[node_id].data.clone();
        arena.new_node(data)
    };

    rv.set(wrap_node(scope, clone_id).into());
}
