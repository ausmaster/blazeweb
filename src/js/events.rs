//! Event system with full WHATWG DOM §2.4 event dispatch.
//!
//! Implements capture→at-target→bubble propagation, stopPropagation,
//! stopImmediatePropagation, composedPath(), and eventPhase.
//!
//! Ported from Servo's event.rs dispatch algorithm.

use std::collections::HashMap;

use crate::dom::arena::NodeId;
use crate::dom::node::NodeData;

// ─── Event phases (WHATWG DOM §2.2) ──────────────────────────────────────────

const PHASE_NONE: i32 = 0;
const PHASE_CAPTURING: i32 = 1;
const PHASE_AT_TARGET: i32 = 2;
const PHASE_BUBBLING: i32 = 3;

// ─── Listener storage ────────────────────────────────────────────────────────

/// Key for looking up event listeners.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ListenerKey {
    Node(NodeId, String),   // (node_id, event_type)
    Window(String),         // (event_type)
    Document(String),       // (event_type) — for document-level listeners
}

/// A single event listener.
struct Listener {
    callback: v8::Global<v8::Function>,
    capture: bool,
    once: bool,
}

/// Event listener storage, kept in an isolate slot.
pub struct EventListenerMap {
    listeners: HashMap<ListenerKey, Vec<Listener>>,
}

impl EventListenerMap {
    pub fn new() -> Self {
        Self {
            listeners: HashMap::new(),
        }
    }

    pub fn add(&mut self, key: ListenerKey, callback: v8::Global<v8::Function>, capture: bool, once: bool) {
        let list = self.listeners.entry(key).or_default();
        // Per spec: deduplicate by (callback, capture) — same function + same capture = skip
        for l in list.iter() {
            if l.callback == callback && l.capture == capture {
                return;
            }
        }
        list.push(Listener { callback, capture, once });
    }

    pub fn remove(&mut self, key: &ListenerKey, callback: &v8::Global<v8::Function>) {
        if let Some(listeners) = self.listeners.get_mut(key) {
            listeners.retain(|l| l.callback != *callback);
            if listeners.is_empty() {
                self.listeners.remove(key);
            }
        }
    }

    /// Snapshot listeners for a key. Returns (callback, capture, once).
    /// Does not modify the listener list — once-removal happens after invoke.
    fn snapshot(&self, key: &ListenerKey) -> Vec<(v8::Global<v8::Function>, bool, bool)> {
        match self.listeners.get(key) {
            Some(list) => list.iter().map(|l| (l.callback.clone(), l.capture, l.once)).collect(),
            None => vec![],
        }
    }

    /// Used by fire_dom_content_loaded (legacy path).
    pub fn take_listeners(&mut self, key: &ListenerKey) -> Vec<(v8::Global<v8::Function>, bool)> {
        if let Some(listeners) = self.listeners.get_mut(key) {
            let result: Vec<_> = listeners.iter().map(|l| (l.callback.clone(), l.once)).collect();
            listeners.retain(|l| !l.once);
            if listeners.is_empty() {
                self.listeners.remove(key);
            }
            result
        } else {
            vec![]
        }
    }
}

// ─── Event path ──────────────────────────────────────────────────────────────

/// An entry in the event propagation path.
#[derive(Clone)]
enum PathEntry {
    Node(NodeId),
    Window,
}

/// Build the event propagation path from target up to Window.
/// Returns [target, parent, ..., document, Window].
/// Shadow-aware: when a node's parent is a shadow root (DocumentFragment),
/// and the event is composed, continues through the shadow host.
fn build_event_path(arena: &crate::dom::Arena, target: NodeId) -> Vec<PathEntry> {
    let mut path = vec![PathEntry::Node(target)];
    let mut current = arena.nodes[target].parent;
    while let Some(id) = current {
        path.push(PathEntry::Node(id));
        // Check if current node is a DocumentFragment (shadow root)
        // If so, find the host element that owns this shadow root
        if matches!(&arena.nodes[id].data, crate::dom::node::NodeData::DocumentFragment) {
            // This might be a shadow root — find the host
            if let Some(host_id) = find_shadow_host(arena, id) {
                path.push(PathEntry::Node(host_id));
                current = arena.nodes[host_id].parent;
                continue;
            }
        }
        current = arena.nodes[id].parent;
    }
    path.push(PathEntry::Window);
    path
}

/// Find the element that hosts a shadow root by searching for elements
/// whose shadow_root field points to the given node.
fn find_shadow_host(arena: &crate::dom::Arena, shadow_id: NodeId) -> Option<NodeId> {
    // Search all elements for one whose shadow_root == shadow_id
    // This is O(n) but shadow roots are rare and event dispatch is infrequent
    for (id, node) in arena.nodes.iter() {
        if let crate::dom::node::NodeData::Element(data) = &node.data {
            if data.shadow_root == Some(shadow_id) {
                return Some(id);
            }
        }
    }
    None
}

/// Convert a path entry to the ListenerKey for looking up listeners.
fn entry_to_key(arena: &crate::dom::Arena, entry: &PathEntry, event_type: &str) -> ListenerKey {
    match entry {
        PathEntry::Node(id) => {
            if matches!(&arena.nodes[*id].data, NodeData::Document) {
                ListenerKey::Document(event_type.to_string())
            } else {
                ListenerKey::Node(*id, event_type.to_string())
            }
        }
        PathEntry::Window => ListenerKey::Window(event_type.to_string()),
    }
}

// ─── V8 property helpers ─────────────────────────────────────────────────────

fn set_prop<'s, 'i>(
    scope: &mut v8::PinnedRef<'s, v8::HandleScope<'i>>,
    obj: v8::Local<v8::Object>,
    key: &str,
    val: v8::Local<'s, v8::Value>,
) {
    let k = v8::String::new(scope, key).unwrap();
    obj.set(scope, k.into(), val);
}

fn get_bool(scope: &mut v8::PinnedRef<v8::HandleScope>, obj: v8::Local<v8::Object>, key: &str) -> bool {
    let k = v8::String::new(scope, key).unwrap();
    obj.get(scope, k.into())
        .map(|v| v.boolean_value(scope))
        .unwrap_or(false)
}

fn get_string(scope: &mut v8::PinnedRef<v8::HandleScope>, obj: v8::Local<v8::Object>, key: &str) -> String {
    let k = v8::String::new(scope, key).unwrap();
    obj.get(scope, k.into())
        .map(|v| v.to_rust_string_lossy(scope))
        .unwrap_or_default()
}

// ─── Propagation flag installation ───────────────────────────────────────────

/// Install working propagation flags and methods on an event object.
/// Called at the start of dispatch — overwrites any existing noop methods.
/// Also called by event constructors so stopPropagation works pre-dispatch.
pub fn install_propagation_flags(scope: &mut v8::PinnedRef<v8::HandleScope>, event: v8::Local<v8::Object>) {
    // Internal flags
    let false_val = v8::Boolean::new(scope, false);
    set_prop(scope, event, "__stopProp", false_val.into());
    set_prop(scope, event, "__stopImm", false_val.into());

    // eventPhase = NONE
    let zero = v8::Integer::new(scope, PHASE_NONE);
    set_prop(scope, event, "eventPhase", zero.into());

    // Working stopPropagation
    let stop_prop = v8::Function::new(scope, |scope: &mut v8::PinnedRef<v8::HandleScope>, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue| {
        let this = args.this();
        let t = v8::Boolean::new(scope, true);
        set_prop(scope, this, "__stopProp", t.into());
    }).unwrap();
    set_prop(scope, event, "stopPropagation", stop_prop.into());

    // Working stopImmediatePropagation
    let stop_imm = v8::Function::new(scope, |scope: &mut v8::PinnedRef<v8::HandleScope>, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue| {
        let this = args.this();
        let t = v8::Boolean::new(scope, true);
        set_prop(scope, this, "__stopProp", t.into());
        set_prop(scope, this, "__stopImm", t.into());
    }).unwrap();
    set_prop(scope, event, "stopImmediatePropagation", stop_imm.into());

    // composedPath — initially returns []; updated during dispatch
    let empty_arr = v8::Array::new(scope, 0);
    set_prop(scope, event, "__composedPath", empty_arr.into());
    let composed_fn = v8::Function::new(scope, |scope: &mut v8::PinnedRef<v8::HandleScope>, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let this = args.this();
        let k = v8::String::new(scope, "__composedPath").unwrap();
        match this.get(scope, k.into()) {
            Some(v) if v.is_array() => rv.set(v),
            _ => {
                let empty = v8::Array::new(scope, 0);
                rv.set(empty.into());
            }
        }
    }).unwrap();
    set_prop(scope, event, "composedPath", composed_fn.into());
}

/// Build the composedPath V8 array from the path entries.
fn build_composed_path_array<'s, 'i>(
    scope: &mut v8::PinnedRef<'s, v8::HandleScope<'i>>,
    path: &[PathEntry],
) -> v8::Local<'s, v8::Array> {
    let arr = v8::Array::new(scope, path.len() as i32);
    for (i, entry) in path.iter().enumerate() {
        let val: v8::Local<v8::Value> = match entry {
            PathEntry::Node(id) => crate::js::templates::wrap_node(scope, *id).into(),
            PathEntry::Window => {
                let ctx = scope.get_current_context();
                ctx.global(scope).into()
            }
        };
        arr.set_index(scope, i as u32, val);
    }
    arr
}

// ─── Core dispatch algorithm ─────────────────────────────────────────────────

/// Invoke listeners for a given key and phase.
/// Returns true if stopImmediatePropagation was called.
fn invoke_listeners(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    event: v8::Local<v8::Object>,
    key: &ListenerKey,
    phase: i32,
) {
    // 1. Snapshot listeners (clones Global refs, releases slot borrow)
    let listeners = {
        let map = scope.get_slot::<EventListenerMap>().unwrap();
        map.snapshot(key)
    };

    if listeners.is_empty() {
        return;
    }

    let mut once_to_remove = Vec::new();

    for (callback, capture, once) in &listeners {
        // Check stopImmediatePropagation
        if get_bool(scope, event, "__stopImm") {
            break;
        }

        // Phase filter:
        // CAPTURING: only capture listeners
        // BUBBLING: only non-capture listeners
        // AT_TARGET: all listeners
        match phase {
            PHASE_CAPTURING if !capture => continue,
            PHASE_BUBBLING if *capture => continue,
            _ => {}
        }

        if *once {
            once_to_remove.push(callback.clone());
        }

        // Call the listener
        crate::try_catch!(let try_catch, scope);
        let func = v8::Local::new(try_catch, callback);
        let undefined = v8::undefined(try_catch);
        let evt = v8::Local::new(try_catch, event);
        func.call(try_catch, undefined.into(), &[evt.into()]);
    }

    // Remove once-listeners
    if !once_to_remove.is_empty() {
        let map = scope.get_slot_mut::<EventListenerMap>().unwrap();
        for cb in &once_to_remove {
            map.remove(key, cb);
        }
    }
}

/// Set event.currentTarget to the V8 wrapper for a path entry.
fn set_current_target(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    event: v8::Local<v8::Object>,
    entry: &PathEntry,
) {
    let val: v8::Local<v8::Value> = match entry {
        PathEntry::Node(id) => crate::js::templates::wrap_node(scope, *id).into(),
        PathEntry::Window => {
            let ctx = scope.get_current_context();
            ctx.global(scope).into()
        }
    };
    set_prop(scope, event, "currentTarget", val);
}

/// Full WHATWG DOM §2.4 event dispatch at a DOM node.
///
/// Builds the event path (target → parent → ... → document → window),
/// then executes capture → at-target → bubble phases.
///
/// Returns true if the event was NOT canceled (i.e., !defaultPrevented).
pub fn dispatch_event_at_node(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    event: v8::Local<v8::Object>,
    target_id: NodeId,
) -> bool {
    // 1. Install propagation support
    install_propagation_flags(scope, event);

    // 2. Build path
    let path = {
        let arena = crate::js::templates::arena_ref(scope);
        build_event_path(arena, target_id)
    };

    // 3. Set event.target = target wrapper
    let target_wrapper = crate::js::templates::wrap_node(scope, target_id);
    set_prop(scope, event, "target", target_wrapper.into());

    // 4. Store composed path
    let composed = build_composed_path_array(scope, &path);
    set_prop(scope, event, "__composedPath", composed.into());

    // 5. Get event type and bubbles flag
    let event_type = get_string(scope, event, "type");
    let bubbles = get_bool(scope, event, "bubbles");

    // 6. CAPTURE PHASE: iterate from outermost (Window) to target's parent
    //    path = [target, parent, ..., document, window]
    //    ancestors = path[1..] = [parent, ..., document, window]
    //    capture order = reversed ancestors = [window, document, ..., parent]
    if path.len() > 1 {
        let ancestors = &path[1..];
        for entry in ancestors.iter().rev() {
            if get_bool(scope, event, "__stopProp") {
                break;
            }
            let phase_val = v8::Integer::new(scope, PHASE_CAPTURING);
            set_prop(scope, event, "eventPhase", phase_val.into());
            set_current_target(scope, event, entry);
            let key = {
                let arena = crate::js::templates::arena_ref(scope);
                entry_to_key(arena, entry, &event_type)
            };
            invoke_listeners(scope, event, &key, PHASE_CAPTURING);
        }
    }

    // 7. AT TARGET PHASE
    if !get_bool(scope, event, "__stopProp") {
        let phase_val = v8::Integer::new(scope, PHASE_AT_TARGET);
        set_prop(scope, event, "eventPhase", phase_val.into());
        set_current_target(scope, event, &path[0]);
        let key = {
            let arena = crate::js::templates::arena_ref(scope);
            entry_to_key(arena, &path[0], &event_type)
        };
        invoke_listeners(scope, event, &key, PHASE_AT_TARGET);
    }

    // 8. BUBBLE PHASE (only if event.bubbles)
    if bubbles && path.len() > 1 {
        let ancestors = &path[1..];
        for entry in ancestors.iter() {
            if get_bool(scope, event, "__stopProp") {
                break;
            }
            let phase_val = v8::Integer::new(scope, PHASE_BUBBLING);
            set_prop(scope, event, "eventPhase", phase_val.into());
            set_current_target(scope, event, entry);
            let key = {
                let arena = crate::js::templates::arena_ref(scope);
                entry_to_key(arena, entry, &event_type)
            };
            invoke_listeners(scope, event, &key, PHASE_BUBBLING);
        }
    }

    // 9. Cleanup
    let none_val = v8::Integer::new(scope, PHASE_NONE);
    set_prop(scope, event, "eventPhase", none_val.into());
    let null = v8::null(scope);
    set_prop(scope, event, "currentTarget", null.into());
    let empty = v8::Array::new(scope, 0);
    set_prop(scope, event, "__composedPath", empty.into());

    !get_bool(scope, event, "defaultPrevented")
}

/// Dispatch an event at the Window level (no bubbling path — Window is the target).
pub fn dispatch_event_at_window(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    event: v8::Local<v8::Object>,
) -> bool {
    install_propagation_flags(scope, event);

    // Target is window (global)
    let ctx = scope.get_current_context();
    let global = ctx.global(scope);
    set_prop(scope, event, "target", global.into());

    // Composed path is just [window]
    let arr = v8::Array::new(scope, 1);
    arr.set_index(scope, 0, global.into());
    set_prop(scope, event, "__composedPath", arr.into());

    // At target only (no ancestors)
    let phase_val = v8::Integer::new(scope, PHASE_AT_TARGET);
    set_prop(scope, event, "eventPhase", phase_val.into());
    set_prop(scope, event, "currentTarget", global.into());

    let event_type = get_string(scope, event, "type");
    let key = ListenerKey::Window(event_type);
    invoke_listeners(scope, event, &key, PHASE_AT_TARGET);

    // Cleanup
    let none_val = v8::Integer::new(scope, PHASE_NONE);
    set_prop(scope, event, "eventPhase", none_val.into());
    let null = v8::null(scope);
    set_prop(scope, event, "currentTarget", null.into());
    let empty = v8::Array::new(scope, 0);
    set_prop(scope, event, "__composedPath", empty.into());

    !get_bool(scope, event, "defaultPrevented")
}

// ─── Event object creation ───────────────────────────────────────────────────

/// Create a minimal Event object for internal use (e.g., DOMContentLoaded).
fn create_event_object<'s, 'i>(
    scope: &mut v8::PinnedRef<'s, v8::HandleScope<'i>>,
    event_type: &str,
    bubbles: bool,
    cancelable: bool,
) -> v8::Local<'s, v8::Object> {
    let obj = v8::Object::new(scope);

    let type_str = v8::String::new(scope, event_type).unwrap();
    set_prop(scope, obj, "type", type_str.into());

    let b = v8::Boolean::new(scope, bubbles);
    set_prop(scope, obj, "bubbles", b.into());

    let c = v8::Boolean::new(scope, cancelable);
    set_prop(scope, obj, "cancelable", c.into());

    let false_val = v8::Boolean::new(scope, false);
    set_prop(scope, obj, "defaultPrevented", false_val.into());

    let true_val = v8::Boolean::new(scope, true);
    set_prop(scope, obj, "isTrusted", true_val.into());

    let null = v8::null(scope);
    set_prop(scope, obj, "target", null.into());
    set_prop(scope, obj, "currentTarget", null.into());

    let zero = v8::Integer::new(scope, 0);
    set_prop(scope, obj, "eventPhase", zero.into());

    // preventDefault
    let prevent = v8::Function::new(scope, |scope: &mut v8::PinnedRef<v8::HandleScope>, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue| {
        let this = args.this();
        if get_bool(scope, this, "cancelable") {
            let t = v8::Boolean::new(scope, true);
            set_prop(scope, this, "defaultPrevented", t.into());
        }
    }).unwrap();
    set_prop(scope, obj, "preventDefault", prevent.into());

    // Install working propagation methods
    install_propagation_flags(scope, obj);

    obj
}

// ─── DOMContentLoaded ────────────────────────────────────────────────────────

/// Fire DOMContentLoaded event on document and window listeners.
/// Returns collected error messages.
pub fn fire_dom_content_loaded(scope: &mut v8::PinnedRef<v8::HandleScope>) -> Vec<String> {
    let mut errors = Vec::new();

    let event = create_event_object(scope, "DOMContentLoaded", true, false);

    // Set target to document
    let doc_key = v8::String::new(scope, "document").unwrap();
    let context = scope.get_current_context();
    let global = context.global(scope);
    if let Some(doc) = global.get(scope, doc_key.into()) {
        set_prop(scope, event, "target", doc);
    }

    // Collect listeners for document + window
    let event_map = scope.get_slot_mut::<EventListenerMap>().unwrap();
    let doc_listeners = event_map.take_listeners(&ListenerKey::Document("DOMContentLoaded".into()));
    let win_listeners = event_map.take_listeners(&ListenerKey::Window("DOMContentLoaded".into()));

    // Fire document listeners first
    for (callback, _once) in doc_listeners {
        crate::try_catch!(let try_catch, scope);
        let func = v8::Local::new(try_catch, &callback);
        let undefined = v8::undefined(try_catch);
        let event_local = v8::Local::new(try_catch, event);
        if func.call(try_catch, undefined.into(), &[event_local.into()]).is_none() {
            if let Some(exc) = try_catch.exception() {
                errors.push(exc.to_rust_string_lossy(try_catch));
            }
        }
    }

    // Fire window listeners
    for (callback, _once) in win_listeners {
        crate::try_catch!(let try_catch, scope);
        let func = v8::Local::new(try_catch, &callback);
        let undefined = v8::undefined(try_catch);
        let event_local = v8::Local::new(try_catch, event);
        if func.call(try_catch, undefined.into(), &[event_local.into()]).is_none() {
            if let Some(exc) = try_catch.exception() {
                errors.push(exc.to_rust_string_lossy(try_catch));
            }
        }
    }

    errors
}

// ─── V8 callbacks ────────────────────────────────────────────────────────────

/// Node.addEventListener() callback.
pub fn add_event_listener_callback(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let node_id = match crate::js::templates::unwrap_node_id(scope, args.this()) {
        Some(id) => id,
        None => return,
    };

    let event_type = args.get(0).to_rust_string_lossy(scope);
    let callback_arg = args.get(1);
    if !callback_arg.is_function() {
        return;
    }
    let func = unsafe { v8::Local::<v8::Function>::cast_unchecked(callback_arg) };
    let global_func = v8::Global::new(scope, func);

    let (capture, once) = parse_listener_options(scope, &args);

    let arena = crate::js::templates::arena_ref(scope);
    let key = if matches!(&arena.nodes[node_id].data, NodeData::Document) {
        ListenerKey::Document(event_type)
    } else {
        ListenerKey::Node(node_id, event_type)
    };

    let event_map = scope.get_slot_mut::<EventListenerMap>().unwrap();
    event_map.add(key, global_func, capture, once);
}

/// Node.removeEventListener() callback.
pub fn remove_event_listener_callback(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let node_id = match crate::js::templates::unwrap_node_id(scope, args.this()) {
        Some(id) => id,
        None => return,
    };

    let event_type = args.get(0).to_rust_string_lossy(scope);
    let callback_arg = args.get(1);
    if !callback_arg.is_function() {
        return;
    }
    let func = unsafe { v8::Local::<v8::Function>::cast_unchecked(callback_arg) };
    let global_func = v8::Global::new(scope, func);

    let arena = crate::js::templates::arena_ref(scope);
    let key = if matches!(&arena.nodes[node_id].data, NodeData::Document) {
        ListenerKey::Document(event_type)
    } else {
        ListenerKey::Node(node_id, event_type)
    };

    let event_map = scope.get_slot_mut::<EventListenerMap>().unwrap();
    event_map.remove(&key, &global_func);
}

/// Node.dispatchEvent(event) — full propagation dispatch.
pub fn dispatch_event_callback(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let node_id = match crate::js::templates::unwrap_node_id(scope, args.this()) {
        Some(id) => id,
        None => return,
    };

    let event_arg = args.get(0);
    if !event_arg.is_object() {
        rv.set(v8::Boolean::new(scope, true).into());
        return;
    }
    let event_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(event_arg) };

    let result = dispatch_event_at_node(scope, event_obj, node_id);
    rv.set(v8::Boolean::new(scope, result).into());
}

/// Window.addEventListener() callback.
pub fn window_add_event_listener(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let callback_arg = args.get(1);
    if !callback_arg.is_function() {
        return;
    }
    let func = unsafe { v8::Local::<v8::Function>::cast_unchecked(callback_arg) };
    let global_func = v8::Global::new(scope, func);

    let (capture, once) = parse_listener_options(scope, &args);

    let key = ListenerKey::Window(event_type);
    let event_map = scope.get_slot_mut::<EventListenerMap>().unwrap();
    event_map.add(key, global_func, capture, once);
}

/// Window.removeEventListener() callback.
pub fn window_remove_event_listener(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let callback_arg = args.get(1);
    if !callback_arg.is_function() {
        return;
    }
    let func = unsafe { v8::Local::<v8::Function>::cast_unchecked(callback_arg) };
    let global_func = v8::Global::new(scope, func);

    let key = ListenerKey::Window(event_type);
    let event_map = scope.get_slot_mut::<EventListenerMap>().unwrap();
    event_map.remove(&key, &global_func);
}

/// Window.dispatchEvent(event) — dispatch at window level.
pub fn window_dispatch_event_callback(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let event_arg = args.get(0);
    if !event_arg.is_object() {
        rv.set(v8::Boolean::new(scope, true).into());
        return;
    }
    let event_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(event_arg) };

    let result = dispatch_event_at_window(scope, event_obj);
    rv.set(v8::Boolean::new(scope, result).into());
}

fn parse_listener_options(scope: &mut v8::PinnedRef<v8::HandleScope>, args: &v8::FunctionCallbackArguments) -> (bool, bool) {
    let opts_arg = args.get(2);
    if opts_arg.is_boolean() {
        return (opts_arg.boolean_value(scope), false);
    }
    if opts_arg.is_object() {
        let obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(opts_arg) };
        let capture = get_bool(scope, obj, "capture");
        let once = get_bool(scope, obj, "once");
        return (capture, once);
    }
    (false, false)
}
