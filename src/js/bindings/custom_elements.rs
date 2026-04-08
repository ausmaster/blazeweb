//! CustomElementRegistry (customElements global) with lifecycle callback support.
//!
//! Stores registered custom element definitions with their lifecycle callbacks
//! (connectedCallback, disconnectedCallback, attributeChangedCallback) and
//! observedAttributes, enabling them to fire on DOM mutations.

use std::collections::HashMap;

/// State stored in the V8 isolate slot for custom element definitions.
pub struct CustomElementState {
    /// Map from tag name → definition
    pub definitions: HashMap<String, CustomElementDefinition>,
}

/// A registered custom element definition.
pub struct CustomElementDefinition {
    /// The constructor function (v8::Global)
    pub constructor: v8::Global<v8::Function>,
    /// connectedCallback (if defined on prototype)
    pub connected_callback: Option<v8::Global<v8::Function>>,
    /// disconnectedCallback (if defined on prototype)
    pub disconnected_callback: Option<v8::Global<v8::Function>>,
    /// attributeChangedCallback (if defined on prototype)
    pub attribute_changed_callback: Option<v8::Global<v8::Function>>,
    /// Static observedAttributes array from constructor
    pub observed_attributes: Vec<String>,
}

/// Install the `customElements` registry on the global object.
pub fn install(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    // Initialize state in isolate slot
    scope.set_slot(CustomElementState {
        definitions: HashMap::new(),
    });

    let obj = v8::Object::new(scope);

    // define(name, constructor, options?)
    let define_fn = v8::Function::new(scope, ce_define).unwrap();
    let k = v8::String::new(scope, "define").unwrap();
    obj.set(scope, k.into(), define_fn.into());

    // get(name)
    let get_fn = v8::Function::new(scope, ce_get).unwrap();
    let k = v8::String::new(scope, "get").unwrap();
    obj.set(scope, k.into(), get_fn.into());

    // whenDefined(name) — returns immediately resolved promise
    let when_fn = v8::Function::new(scope, ce_when_defined).unwrap();
    let k = v8::String::new(scope, "whenDefined").unwrap();
    obj.set(scope, k.into(), when_fn.into());

    // upgrade(element) — no-op for SSR
    let noop = v8::Function::new(scope, |_: &mut v8::HandleScope,
        _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    let k = v8::String::new(scope, "upgrade").unwrap();
    obj.set(scope, k.into(), noop.into());

    let key = v8::String::new(scope, "customElements").unwrap();
    global.set(scope, key.into(), obj.into());

    // CustomElementRegistry constructor — for instanceof checks and polyfill detection.
    // The constructor throws Illegal constructor; the prototype has the registry methods.
    let source = r#"
    (function(g) {
        function CustomElementRegistry() {
            throw new TypeError("Illegal constructor");
        }
        // Copy methods from the customElements instance to the prototype
        var ce = g.customElements;
        CustomElementRegistry.prototype.define = function(n,c,o){return ce.define(n,c,o);};
        CustomElementRegistry.prototype.get = function(n){return ce.get(n);};
        CustomElementRegistry.prototype.whenDefined = function(n){return ce.whenDefined(n);};
        CustomElementRegistry.prototype.upgrade = function(e){return ce.upgrade(e);};
        // Make customElements an instance of CustomElementRegistry
        Object.setPrototypeOf(ce, CustomElementRegistry.prototype);
        g.CustomElementRegistry = CustomElementRegistry;
    })(self)
    "#;
    super::window::run_js(scope, source, "[blazeweb:CustomElementRegistry]");

    log::debug!("Installed customElements registry with lifecycle support + CustomElementRegistry constructor");
}

/// Install the construction stack for custom element upgrade.
/// MUST be called AFTER HTMLElement constructor is registered on the global.
pub fn install_construction_stack(scope: &mut v8::HandleScope, _global: v8::Local<v8::Object>) {
    // Construction stack + HTMLElement replacement for custom element upgrade.
    // When define() is called, existing elements get upgraded by invoking their
    // constructor. super() calls HTMLElement() which pops the construction stack
    // and returns the existing element as `this`.
    let ce_stack_source = r#"
    (function(g) {
        g.__ceConstructionStack = [];
        var OrigHTMLElement = g.HTMLElement;
        var origProto = OrigHTMLElement.prototype;
        g.HTMLElement = function HTMLElement() {
            var stack = g.__ceConstructionStack;
            if (stack.length > 0) return stack.pop();
            throw new TypeError("Illegal constructor");
        };
        g.HTMLElement.prototype = origProto;
        origProto.constructor = g.HTMLElement;

        // Upgrade helper: push element onto stack, call constructor via Reflect.construct
        g.__ceUpgrade = function(element, Constructor) {
            g.__ceConstructionStack.push(element);
            try {
                Reflect.construct(Constructor, [], Constructor);
            } catch(e) {
                // Remove element from stack on failure
                var idx = g.__ceConstructionStack.indexOf(element);
                if (idx >= 0) g.__ceConstructionStack.splice(idx, 1);
                // Don't rethrow — upgrade failures should not stop other upgrades
                if (typeof console !== "undefined") {
                    var stack = e.stack ? e.stack.split("\n").slice(0, 6).join(" | ") : "";
                    console.error("CE upgrade error: Constructing " + element.tagName + ": " + e.message + (stack ? " STACK: " + stack : ""));
                }
            }
        };
    })(self)
    "#;
    super::window::run_js(scope, ce_stack_source, "[blazeweb:CE-construction-stack]");
    log::debug!("Installed custom element construction stack + HTMLElement replacement");
}

/// customElements.define(name, constructor, options?)
fn ce_define(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let name = args.get(0).to_rust_string_lossy(scope);
    let ctor_val = args.get(1);
    if !ctor_val.is_function() {
        log::warn!("customElements.define('{}') called with non-function constructor", name);
        return;
    }
    let ctor = unsafe { v8::Local::<v8::Function>::cast_unchecked(ctor_val) };

    // Extract lifecycle callbacks from constructor's prototype
    let proto_key = v8::String::new(scope, "prototype").unwrap();
    let proto_val = ctor.get(scope, proto_key.into());

    let mut connected: Option<v8::Global<v8::Function>> = None;
    let mut disconnected: Option<v8::Global<v8::Function>> = None;
    let mut attr_changed: Option<v8::Global<v8::Function>> = None;

    if let Some(proto) = proto_val {
        if proto.is_object() {
            let proto_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(proto) };
            connected = extract_callback(scope, proto_obj, "connectedCallback");
            disconnected = extract_callback(scope, proto_obj, "disconnectedCallback");
            attr_changed = extract_callback(scope, proto_obj, "attributeChangedCallback");
        }
    }

    // Extract static observedAttributes from constructor
    let mut observed_attrs = Vec::new();
    let oa_key = v8::String::new(scope, "observedAttributes").unwrap();
    if let Some(oa_val) = ctor.get(scope, oa_key.into()) {
        if oa_val.is_array() {
            let oa_arr = unsafe { v8::Local::<v8::Array>::cast_unchecked(oa_val) };
            for i in 0..oa_arr.length() {
                if let Some(item) = oa_arr.get_index(scope, i) {
                    observed_attrs.push(item.to_rust_string_lossy(scope));
                }
            }
        }
    }

    log::debug!(
        "customElements.define('{}') — connected={} disconnected={} attrChanged={} observedAttrs={:?}",
        name,
        connected.is_some(),
        disconnected.is_some(),
        attr_changed.is_some(),
        observed_attrs
    );

    // Store the global constructor
    let ctor_global = v8::Global::new(scope, ctor);

    // Check if already defined
    let already = scope.get_slot::<CustomElementState>()
        .map(|s| s.definitions.contains_key(&name))
        .unwrap_or(false);
    if already {
        let msg = v8::String::new(scope, &format!(
            "Failed to execute 'define': the name \"{}\" has already been used", name
        )).unwrap();
        let exc = v8::Exception::error(scope, msg);
        scope.throw_exception(exc);
        return;
    }

    // Store definition in isolate slot
    let name_clone = name.clone();
    if let Some(state) = scope.get_slot_mut::<CustomElementState>() {
        state.definitions.insert(name.clone(), CustomElementDefinition {
            constructor: ctor_global,
            connected_callback: connected,
            disconnected_callback: disconnected,
            attribute_changed_callback: attr_changed,
            observed_attributes: observed_attrs,
        });
    }

    // ─── Custom Element Upgrade ─────────────────────────────────────────
    // Walk the DOM tree and upgrade all existing elements matching this tag name.
    // Per spec: "enqueue a custom element upgrade reaction" for each matching element.
    // For SSR we do this synchronously.
    let matching = {
        let arena = crate::js::templates::arena_ref(scope);
        let mut found = Vec::new();
        collect_matching_elements(arena, arena.document, &name_clone, &mut found);
        found
    };

    if !matching.is_empty() {
        log::debug!(
            "customElements.define('{}') — upgrading {} existing element(s)",
            name_clone, matching.len()
        );
        for node_id in matching {
            upgrade_element(scope, node_id, &name_clone);
        }
    }
}

/// Extract a named callback function from a prototype object.
fn extract_callback(
    scope: &mut v8::HandleScope,
    proto: v8::Local<v8::Object>,
    name: &str,
) -> Option<v8::Global<v8::Function>> {
    let key = v8::String::new(scope, name).unwrap();
    let val = proto.get(scope, key.into())?;
    if val.is_function() {
        let func = unsafe { v8::Local::<v8::Function>::cast_unchecked(val) };
        Some(v8::Global::new(scope, func))
    } else {
        None
    }
}

/// customElements.get(name) — returns constructor or undefined
fn ce_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let name = args.get(0).to_rust_string_lossy(scope);
    let ctor_global = scope.get_slot::<CustomElementState>()
        .and_then(|state| state.definitions.get(&name))
        .map(|def| def.constructor.clone());

    if let Some(ref g) = ctor_global {
        rv.set(v8::Local::new(scope, g).into());
    } else {
        rv.set(v8::undefined(scope).into());
    }
}

/// customElements.whenDefined(name) — returns immediately resolved promise
fn ce_when_defined(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let resolver = v8::PromiseResolver::new(scope).unwrap();
    let undef = v8::undefined(scope);
    resolver.resolve(scope, undef.into());
    rv.set(resolver.get_promise(scope).into());
}

// ─── Public helpers for DOM mutation hooks ────────────────────────────────

/// Check if a tag name is a custom element (contains hyphen per spec).
#[inline]
pub fn is_custom_element_name(tag: &str) -> bool {
    tag.contains('-')
}

/// Fire connectedCallback for a node if it's a registered custom element.
pub fn fire_connected_callback(
    scope: &mut v8::HandleScope,
    node_obj: v8::Local<v8::Object>,
    tag: &str,
) {
    // Clone the Global callback to release the slot borrow before calling
    let cb_global = scope.get_slot::<CustomElementState>()
        .and_then(|state| state.definitions.get(tag))
        .and_then(|def| def.connected_callback.clone());

    if let Some(ref cb_g) = cb_global {
        let cb = v8::Local::new(scope, cb_g);
        log::trace!("Firing connectedCallback for <{}>", tag);
        let try_catch = &mut v8::TryCatch::new(scope);
        let args: &[v8::Local<v8::Value>] = &[];
        if cb.call(try_catch, node_obj.into(), args).is_none() {
            if let Some(exc) = try_catch.exception() {
                log::warn!("connectedCallback error for <{}>: {}", tag, exc.to_rust_string_lossy(try_catch));
            }
        }
    }
}

/// Fire disconnectedCallback for a node if it's a registered custom element.
pub fn fire_disconnected_callback(
    scope: &mut v8::HandleScope,
    node_obj: v8::Local<v8::Object>,
    tag: &str,
) {
    let cb_global = scope.get_slot::<CustomElementState>()
        .and_then(|state| state.definitions.get(tag))
        .and_then(|def| def.disconnected_callback.clone());

    if let Some(ref cb_g) = cb_global {
        let cb = v8::Local::new(scope, cb_g);
        log::trace!("Firing disconnectedCallback for <{}>", tag);
        let try_catch = &mut v8::TryCatch::new(scope);
        let args: &[v8::Local<v8::Value>] = &[];
        if cb.call(try_catch, node_obj.into(), args).is_none() {
            if let Some(exc) = try_catch.exception() {
                log::warn!("disconnectedCallback error for <{}>: {}", tag, exc.to_rust_string_lossy(try_catch));
            }
        }
    }
}

/// Fire attributeChangedCallback if the attribute is in observedAttributes.
pub fn fire_attribute_changed_callback(
    scope: &mut v8::HandleScope,
    node_obj: v8::Local<v8::Object>,
    tag: &str,
    attr_name: &str,
    old_value: Option<&str>,
    new_value: Option<&str>,
) {
    // Check observed attributes and clone callback in one slot access
    let cb_global = scope.get_slot::<CustomElementState>()
        .and_then(|state| state.definitions.get(tag))
        .and_then(|def| {
            if def.observed_attributes.iter().any(|a| a == attr_name) {
                def.attribute_changed_callback.clone()
            } else {
                None
            }
        });

    if let Some(ref cb_g) = cb_global {
        let cb = v8::Local::new(scope, cb_g);
        log::trace!("Firing attributeChangedCallback for <{}> attr={}", tag, attr_name);
        let try_catch = &mut v8::TryCatch::new(scope);
        let name_val = v8::String::new(try_catch, attr_name).unwrap();
        let old_val: v8::Local<v8::Value> = match old_value {
            Some(s) => v8::String::new(try_catch, s).unwrap().into(),
            None => v8::null(try_catch).into(),
        };
        let new_val: v8::Local<v8::Value> = match new_value {
            Some(s) => v8::String::new(try_catch, s).unwrap().into(),
            None => v8::null(try_catch).into(),
        };
        let args: &[v8::Local<v8::Value>] = &[name_val.into(), old_val, new_val];
        if cb.call(try_catch, node_obj.into(), args).is_none() {
            if let Some(exc) = try_catch.exception() {
                log::warn!("attributeChangedCallback error for <{}>: {}", tag, exc.to_rust_string_lossy(try_catch));
            }
        }
    }
}

// ─── Custom Element Upgrade ─────────────────────────────────────────────────

/// Recursively collect all elements matching a tag name in tree order.
fn collect_matching_elements(
    arena: &crate::dom::Arena,
    node: crate::dom::NodeId,
    tag: &str,
    out: &mut Vec<crate::dom::NodeId>,
) {
    for child in arena.children(node) {
        if let crate::dom::node::NodeData::Element(data) = &arena.nodes[child].data {
            if &*data.name.local == tag {
                out.push(child);
            }
            // Also search inside shadow roots
            if let Some(shadow_id) = data.shadow_root {
                collect_matching_elements(arena, shadow_id, tag, out);
            }
        }
        collect_matching_elements(arena, child, tag, out);
    }
}

/// Upgrade a single custom element: invoke constructor, set prototype, fire callbacks.
fn upgrade_element(
    scope: &mut v8::HandleScope,
    node_id: crate::dom::NodeId,
    tag_name: &str,
) {
    // 1. Clone the constructor + callbacks (release slot borrow before calling JS)
    let (ctor_global, connected_cb, attr_changed_cb, observed_attrs) = {
        let Some(state) = scope.get_slot::<CustomElementState>() else { return };
        let Some(def) = state.definitions.get(tag_name) else { return };
        (
            def.constructor.clone(),
            def.connected_callback.clone(),
            def.attribute_changed_callback.clone(),
            def.observed_attributes.clone(),
        )
    };

    // 2. Get the existing element wrapper
    let element_wrapper = crate::js::templates::wrap_node(scope, node_id);

    // 3. Set prototype to Constructor.prototype
    let ctor_local = v8::Local::new(scope, &ctor_global);
    let proto_key = v8::String::new(scope, "prototype").unwrap();
    if let Some(ctor_proto) = ctor_local.get(scope, proto_key.into()) {
        if ctor_proto.is_object() {
            element_wrapper.set_prototype(scope, ctor_proto);
        }
    }

    // 4. Call __ceUpgrade(element, Constructor) — pushes to construction stack,
    //    invokes Reflect.construct, super() returns the existing element.
    {
        let global = scope.get_current_context().global(scope);
        let upgrade_key = v8::String::new(scope, "__ceUpgrade").unwrap();
        if let Some(upgrade_fn_val) = global.get(scope, upgrade_key.into()) {
            if upgrade_fn_val.is_function() {
                let upgrade_fn = unsafe { v8::Local::<v8::Function>::cast_unchecked(upgrade_fn_val) };
                let undef = v8::undefined(scope);
                let ctor_local2 = v8::Local::new(scope, &ctor_global);
                let try_catch = &mut v8::TryCatch::new(scope);
                if upgrade_fn.call(try_catch, undef.into(), &[element_wrapper.into(), ctor_local2.into()]).is_none() {
                    if let Some(exc) = try_catch.exception() {
                        log::warn!(
                            "Custom element upgrade failed for <{}>: {}",
                            tag_name, exc.to_rust_string_lossy(try_catch)
                        );
                    }
                    return;
                }
            }
        }
    }

    // 5. Fire attributeChangedCallback for existing observed attributes
    if let Some(ref attr_cb_global) = attr_changed_cb {
        let attrs_to_fire: Vec<(String, String)> = {
            let arena = crate::js::templates::arena_ref(scope);
            if let crate::dom::node::NodeData::Element(data) = &arena.nodes[node_id].data {
                data.attrs.iter()
                    .filter(|a| observed_attrs.iter().any(|oa| oa == &*a.name.local))
                    .map(|a| (a.name.local.to_string(), a.value.to_string()))
                    .collect()
            } else {
                vec![]
            }
        };

        for (attr_name, attr_value) in attrs_to_fire {
            let cb = v8::Local::new(scope, attr_cb_global);
            let try_catch = &mut v8::TryCatch::new(scope);
            let name_val = v8::String::new(try_catch, &attr_name).unwrap();
            let null_val = v8::null(try_catch);
            let new_val = v8::String::new(try_catch, &attr_value).unwrap();
            if cb.call(try_catch, element_wrapper.into(), &[name_val.into(), null_val.into(), new_val.into()]).is_none() {
                if let Some(exc) = try_catch.exception() {
                    log::warn!("attributeChangedCallback error during upgrade of <{}>: {}", tag_name, exc.to_rust_string_lossy(try_catch));
                }
            }
        }
    }

    // 6. Fire connectedCallback if element is connected to the document
    let is_connected = {
        let arena = crate::js::templates::arena_ref(scope);
        arena.nodes[node_id].flags.is_connected()
    };
    if is_connected {
        if let Some(ref cc_global) = connected_cb {
            let cb = v8::Local::new(scope, cc_global);
            let try_catch = &mut v8::TryCatch::new(scope);
            if cb.call(try_catch, element_wrapper.into(), &[]).is_none() {
                if let Some(exc) = try_catch.exception() {
                    log::warn!("connectedCallback error during upgrade of <{}>: {}", tag_name, exc.to_rust_string_lossy(try_catch));
                }
            }
        }
    }

    // Flush microtasks after upgrade — constructor may have scheduled async work
    scope.perform_microtask_checkpoint();

    log::trace!("Upgraded custom element <{}>", tag_name);
}
