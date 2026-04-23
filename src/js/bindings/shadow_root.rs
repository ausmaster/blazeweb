/// Shadow DOM: ShadowRoot interface, Element.attachShadow(), Element.shadowRoot getter.
///
/// ShadowRoot is a DocumentFragment in the arena with shadow-specific metadata
/// (mode, host) stored on the V8 wrapper. Uses JS-defined class for proper
/// instanceof support.

use crate::dom::node::NodeData;
use crate::js::templates::{arena_mut, arena_ref, unwrap_node_id, wrap_node};

/// Valid shadow host tag names per spec (HTML namespace only).
/// Custom elements (tags with hyphens) are always valid.
const VALID_SHADOW_HOSTS: &[&str] = &[
    "article", "aside", "blockquote", "body", "div", "footer",
    "h1", "h2", "h3", "h4", "h5", "h6",
    "header", "main", "nav", "p", "section", "span",
];

/// Install the ShadowRoot global constructor (throws Illegal constructor).
pub fn install(scope: &mut v8::PinnedRef<v8::HandleScope>, global: v8::Local<v8::Object>) {
    // ShadowRoot constructor — we can't use class extends because DocumentFragment
    // isn't a real JS class we can extend. Instead, create a constructor function
    // and set its prototype chain manually after we create the shadow root instance.
    let source = r#"
    (function(g) {
        function ShadowRoot() {
            throw new TypeError("Illegal constructor");
        }
        g.ShadowRoot = ShadowRoot;
        g.__ShadowRootProto = ShadowRoot.prototype;
        return ShadowRoot;
    })(self)
    "#;
    let source_str = v8::String::new(scope, source).unwrap();
    let name = v8::String::new(scope, "[blazeweb:ShadowRoot]").unwrap();
    let origin = v8::ScriptOrigin::new(
        scope, name.into(), 0, 0, false, -1, None, false, false, false, None,
    );
    if let Some(script) = v8::Script::compile(scope, source_str, Some(&origin)) {
        if script.run(scope).is_none() {
            log::error!("Failed to install ShadowRoot class");
        }
    }
    log::debug!("Installed ShadowRoot constructor");
    let _ = global; // used indirectly via self
}

/// Implementation of Element.attachShadow(options).
/// Called from element_geometry.rs.
pub fn attach_shadow(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(host_id) = unwrap_node_id(scope, args.this()) else { return };

    // 1. Validate options argument
    let options = args.get(0);
    if !options.is_object() {
        let msg = v8::String::new(scope, "Failed to execute 'attachShadow': 1 argument required").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    }
    let options_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(options) };

    // 2. Extract mode
    let mode_key = v8::String::new(scope, "mode").unwrap();
    let mode_val = options_obj.get(scope, mode_key.into());
    let mode = mode_val
        .map(|v| v.to_rust_string_lossy(scope))
        .unwrap_or_default();
    if mode != "open" && mode != "closed" {
        let msg = v8::String::new(scope, "Failed to execute 'attachShadow': The provided value 'mode' is not a valid enum value").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    }

    // 3. Validate host element
    let arena = arena_ref(scope);
    let valid = if let NodeData::Element(data) = &arena.nodes[host_id].data {
        let tag = &*data.name.local;
        // Custom elements (contain hyphen) are always valid
        tag.contains('-') || VALID_SHADOW_HOSTS.contains(&tag)
    } else {
        false
    };
    if !valid {
        let msg = v8::String::new(scope, "Failed to execute 'attachShadow': This element does not support attachShadow").unwrap();
        let exc = v8::Exception::error(scope, msg);
        scope.throw_exception(exc);
        return;
    }

    // 4. Check no existing shadow root
    if let NodeData::Element(data) = &arena.nodes[host_id].data {
        if data.shadow_root.is_some() {
            let msg = v8::String::new(scope, "Failed to execute 'attachShadow': Shadow root cannot be created on a host which already hosts a shadow tree").unwrap();
            let exc = v8::Exception::error(scope, msg);
            scope.throw_exception(exc);
            return;
        }
    }

    // 5. Create shadow root as DocumentFragment in arena
    let arena = arena_mut(scope);
    let shadow_id = arena.new_node(NodeData::DocumentFragment);

    // Store shadow root on host element
    if let NodeData::Element(data) = &mut arena.nodes[host_id].data {
        data.shadow_root = Some(shadow_id);
    }

    log::debug!("attachShadow(mode={}) on {:?} -> shadow {:?}", mode, host_id, shadow_id);

    // 6. Wrap the shadow root node (gets Node prototype methods automatically)
    let shadow_obj = wrap_node(scope, shadow_id);

    // 7. Set ShadowRoot prototype for instanceof support
    // We set ShadowRoot.prototype.__proto__ to the shadow object's current prototype
    // (which has all Node/Document methods from the template). Then set the shadow
    // object's prototype to ShadowRoot.prototype. This gives us:
    //   shadow_obj -> ShadowRoot.prototype -> Node/Document prototype -> Object.prototype
    let global = scope.get_current_context().global(scope);
    let proto_key = v8::String::new(scope, "__ShadowRootProto").unwrap();
    if let Some(sr_proto) = global.get(scope, proto_key.into()) {
        if sr_proto.is_object() {
            let sr_proto_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(sr_proto) };
            // Make ShadowRoot.prototype inherit from the node's current prototype
            // (only set once — check if already done)
            let already_key = v8::String::new(scope, "__sr_proto_set").unwrap();
            let already = sr_proto_obj.get(scope, already_key.into())
                .map(|v| v.is_true())
                .unwrap_or(false);
            if !already {
                if let Some(node_proto) = shadow_obj.get_prototype(scope) {
                    sr_proto_obj.set_prototype(scope, node_proto);
                }
                let t = v8::Boolean::new(scope, true);
                sr_proto_obj.set(scope, already_key.into(), t.into());
            }
            shadow_obj.set_prototype(scope, sr_proto_obj.into());
        }
    }

    // 8. Set shadow-specific properties
    let k = v8::String::new(scope, "mode").unwrap();
    let v = v8::String::new(scope, &mode).unwrap();
    shadow_obj.set(scope, k.into(), v.into());

    // host — reference to the host element
    let host_obj = wrap_node(scope, host_id);
    let k = v8::String::new(scope, "host").unwrap();
    shadow_obj.set(scope, k.into(), host_obj.into());

    // innerHTML getter/setter
    install_innerhtml(scope, shadow_obj, shadow_id);

    // querySelector/querySelectorAll
    install_query_selectors(scope, shadow_obj);

    // getElementById
    install_get_element_by_id(scope, shadow_obj);

    // children (HTMLCollection-like with .item())
    install_children(scope, shadow_obj);

    // adoptedStyleSheets (empty array)
    let k = v8::String::new(scope, "adoptedStyleSheets").unwrap();
    let arr = v8::Array::new(scope, 0);
    shadow_obj.set(scope, k.into(), arr.into());

    // Store mode on the host wrapper for shadowRoot getter
    let mode_prop = v8::String::new(scope, "__shadowMode").unwrap();
    let mode_v = v8::String::new(scope, &mode).unwrap();
    host_obj.set(scope, mode_prop.into(), mode_v.into());

    // Store shadow root global reference on host for shadowRoot getter
    let sr_prop = v8::String::new(scope, "__shadowRoot").unwrap();
    host_obj.set(scope, sr_prop.into(), shadow_obj.into());

    rv.set(shadow_obj.into());
}

/// Element.shadowRoot getter — returns shadow root for open mode, null for closed.
pub fn shadow_root_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();

    // Check mode
    let mode_key = v8::String::new(scope, "__shadowMode").unwrap();
    let mode = this.get(scope, mode_key.into())
        .filter(|v| !v.is_undefined() && !v.is_null())
        .map(|v| v.to_rust_string_lossy(scope));

    match mode.as_deref() {
        Some("open") => {
            // Return the stored shadow root
            let sr_key = v8::String::new(scope, "__shadowRoot").unwrap();
            if let Some(sr) = this.get(scope, sr_key.into()) {
                if !sr.is_undefined() && !sr.is_null() {
                    rv.set(sr);
                    return;
                }
            }
            rv.set(v8::null(scope).into());
        }
        Some("closed") => {
            rv.set(v8::null(scope).into());
        }
        _ => {
            rv.set(v8::null(scope).into());
        }
    }
}

// ─── innerHTML on ShadowRoot ────────────────────────────────────────────────

fn install_innerhtml(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    shadow_obj: v8::Local<v8::Object>,
    _shadow_id: crate::dom::NodeId,
) {
    // innerHTML getter — serialize shadow root's children
    let getter = v8::Function::new(scope, innerhtml_getter).unwrap();

    // innerHTML setter — parse HTML and replace children
    let setter = v8::Function::new(scope, innerhtml_setter).unwrap();

    // Use Object.defineProperty for getter/setter pair
    let source = v8::String::new(scope, r#"
    (function(obj, get, set) {
        Object.defineProperty(obj, "innerHTML", {
            get: function() { return get.call(this); },
            set: function(v) { return set.call(this, v); },
            configurable: true,
            enumerable: true
        });
    })
    "#).unwrap();
    let name = v8::String::new(scope, "[blazeweb:sr-innerHTML]").unwrap();
    let origin = v8::ScriptOrigin::new(
        scope, name.into(), 0, 0, false, -1, None, false, false, false, None,
    );
    if let Some(script) = v8::Script::compile(scope, source, Some(&origin)) {
        if let Some(define_fn) = script.run(scope) {
            if define_fn.is_function() {
                let func = unsafe { v8::Local::<v8::Function>::cast_unchecked(define_fn) };
                let undef = v8::undefined(scope);
                func.call(scope, undef.into(), &[shadow_obj.into(), getter.into(), setter.into()]);
            }
        }
    }
}

fn innerhtml_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let mut output = String::new();
    for child in arena.children(node_id) {
        crate::dom::serialize::serialize_node_to_string(arena, child, &mut output);
    }
    let html = output;
    let v = v8::String::new(scope, &html).unwrap();
    rv.set(v.into());
}

fn innerhtml_setter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let html_str = args.get(0).to_rust_string_lossy(scope);

    // Parse the HTML fragment
    let fragment_arena = crate::dom::treesink::parse_fragment(&html_str, "div", true);

    let arena = arena_mut(scope);
    // Remove existing children
    arena.remove_all_children(node_id);

    // Clone parsed nodes into our arena
    if let Some(html_wrapper) = fragment_arena.children(fragment_arena.document).next() {
        for child in fragment_arena.children(html_wrapper) {
            let new_id = super::element::clone_across_arenas(arena, &fragment_arena, child);
            arena.append_child(node_id, new_id);
        }
    }
    log::trace!("ShadowRoot.innerHTML set ({} bytes)", html_str.len());
}

// ─── querySelector/querySelectorAll on ShadowRoot ───────────────────────────

fn install_query_selectors(scope: &mut v8::PinnedRef<v8::HandleScope>, shadow_obj: v8::Local<v8::Object>) {
    let qs = v8::Function::new(scope, shadow_query_selector).unwrap();
    let k = v8::String::new(scope, "querySelector").unwrap();
    shadow_obj.set(scope, k.into(), qs.into());

    let qsa = v8::Function::new(scope, shadow_query_selector_all).unwrap();
    let k = v8::String::new(scope, "querySelectorAll").unwrap();
    shadow_obj.set(scope, k.into(), qsa.into());
}

fn shadow_query_selector(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(root_id) = unwrap_node_id(scope, args.this()) else { return };
    let selector_str = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_ref(scope);

    // Reuse the selector matching from element.rs
    if let Ok(result) = crate::dom::selector::query_selector(arena, root_id, &selector_str) {
        if let Some(found_id) = result {
            rv.set(wrap_node(scope, found_id).into());
            return;
        }
    }
    rv.set(v8::null(scope).into());
}

fn shadow_query_selector_all(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(root_id) = unwrap_node_id(scope, args.this()) else { return };
    let selector_str = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_ref(scope);

    let results = crate::dom::selector::query_selector_all(arena, root_id, &selector_str)
        .unwrap_or_default();
    let arr = v8::Array::new(scope, results.len() as i32);
    for (i, id) in results.iter().enumerate() {
        let wrapped = wrap_node(scope, *id);
        arr.set_index(scope, i as u32, wrapped.into());
    }
    super::element::add_item_method(scope, arr);
    rv.set(arr.into());
}

// ─── getElementById on ShadowRoot ───────────────────────────────────────────

fn install_get_element_by_id(scope: &mut v8::PinnedRef<v8::HandleScope>, shadow_obj: v8::Local<v8::Object>) {
    let func = v8::Function::new(scope, shadow_get_element_by_id).unwrap();
    let k = v8::String::new(scope, "getElementById").unwrap();
    shadow_obj.set(scope, k.into(), func.into());
}

fn shadow_get_element_by_id(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(root_id) = unwrap_node_id(scope, args.this()) else { return };
    let id_str = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_ref(scope);

    // Walk descendants looking for id match
    fn find_by_id(arena: &crate::dom::Arena, parent: crate::dom::NodeId, id: &str) -> Option<crate::dom::NodeId> {
        for child in arena.children(parent) {
            if let NodeData::Element(data) = &arena.nodes[child].data {
                if data.get_attribute("id") == Some(id) {
                    return Some(child);
                }
            }
            if let Some(found) = find_by_id(arena, child, id) {
                return Some(found);
            }
        }
        None
    }

    if let Some(found) = find_by_id(arena, root_id, &id_str) {
        rv.set(wrap_node(scope, found).into());
    } else {
        rv.set(v8::null(scope).into());
    }
}

// ─── children (HTMLCollection) on ShadowRoot ────────────────────────────────

fn install_children(scope: &mut v8::PinnedRef<v8::HandleScope>, shadow_obj: v8::Local<v8::Object>) {
    // children getter — element children only
    let getter = v8::Function::new(scope, shadow_children_getter).unwrap();

    let source = v8::String::new(scope, r#"
    (function(obj, get) {
        Object.defineProperty(obj, "children", {
            get: function() { return get.call(this); },
            configurable: true,
            enumerable: true
        });
    })
    "#).unwrap();
    let name = v8::String::new(scope, "[blazeweb:sr-children]").unwrap();
    let origin = v8::ScriptOrigin::new(
        scope, name.into(), 0, 0, false, -1, None, false, false, false, None,
    );
    if let Some(script) = v8::Script::compile(scope, source, Some(&origin)) {
        if let Some(define_fn) = script.run(scope) {
            if define_fn.is_function() {
                let func = unsafe { v8::Local::<v8::Function>::cast_unchecked(define_fn) };
                let undef = v8::undefined(scope);
                func.call(scope, undef.into(), &[shadow_obj.into(), getter.into()]);
            }
        }
    }
}

fn shadow_children_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
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
    super::element::add_item_method(scope, arr);
    rv.set(arr.into());
}
