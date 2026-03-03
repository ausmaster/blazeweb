/// Observer constructors: MutationObserver (real), IntersectionObserver, ResizeObserver (stubs).

use crate::js::mutation_observer::{MutationObserverState, ObserverOptions};
use crate::js::templates::unwrap_node_id;

/// Install observer constructors on the global object.
pub fn install(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    let mo = v8::Function::new(scope, mutation_observer_constructor).unwrap();
    let key = v8::String::new(scope, "MutationObserver").unwrap();
    global.set(scope, key.into(), mo.into());

    let io = v8::Function::new(scope, intersection_observer_constructor).unwrap();
    let key = v8::String::new(scope, "IntersectionObserver").unwrap();
    global.set(scope, key.into(), io.into());

    let ro = v8::Function::new(scope, resize_observer_constructor).unwrap();
    let key = v8::String::new(scope, "ResizeObserver").unwrap();
    global.set(scope, key.into(), ro.into());
}

// ─── MutationObserver (real implementation) ──────────────────────────────────

fn mutation_observer_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let callback_arg = args.get(0);
    if !callback_arg.is_function() {
        let msg = v8::String::new(
            scope,
            "Failed to construct 'MutationObserver': The callback provided as parameter 1 is not a function.",
        ).unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    }
    let callback = unsafe { v8::Local::<v8::Function>::cast_unchecked(callback_arg) };
    let callback_global = v8::Global::new(scope, callback);

    // Create the JS observer object
    let obj = v8::Object::new(scope);
    let obj_global = v8::Global::new(scope, obj);

    // Register in centralized state
    let observer_idx = {
        let state = scope.get_slot_mut::<MutationObserverState>().unwrap();
        state.add_observer(callback_global, obj_global)
    };

    // Store observer_idx as private property
    let name = v8::String::new(scope, "__mo_idx").unwrap();
    let idx_key = v8::Private::for_api(scope, Some(name));
    let idx_val = v8::Integer::new(scope, observer_idx as i32);
    obj.set_private(scope, idx_key, idx_val.into());

    // observe(target, options)
    let observe_fn = v8::Function::new(scope, mo_observe).unwrap();
    let k = v8::String::new(scope, "observe").unwrap();
    obj.set(scope, k.into(), observe_fn.into());

    // disconnect()
    let disconnect_fn = v8::Function::new(scope, mo_disconnect).unwrap();
    let k = v8::String::new(scope, "disconnect").unwrap();
    obj.set(scope, k.into(), disconnect_fn.into());

    // takeRecords()
    let take_fn = v8::Function::new(scope, mo_take_records).unwrap();
    let k = v8::String::new(scope, "takeRecords").unwrap();
    obj.set(scope, k.into(), take_fn.into());

    rv.set(obj.into());
}

/// Extract observer index from a MO JS object via private property.
fn get_observer_idx(scope: &mut v8::HandleScope, this: v8::Local<v8::Object>) -> Option<usize> {
    let name = v8::String::new(scope, "__mo_idx").unwrap();
    let idx_key = v8::Private::for_api(scope, Some(name));
    let val = this.get_private(scope, idx_key)?;
    Some(val.int32_value(scope)? as usize)
}

fn mo_observe(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let this = args.this();
    let Some(observer_idx) = get_observer_idx(scope, this) else { return };

    // Arg 0: target node
    let target_arg = args.get(0);
    if !target_arg.is_object() {
        let msg = v8::String::new(scope, "Failed to execute 'observe' on 'MutationObserver': parameter 1 is not of type 'Node'.").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    }
    let target_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(target_arg) };
    let Some(target_id) = unwrap_node_id(scope, target_obj) else {
        let msg = v8::String::new(scope, "Failed to execute 'observe' on 'MutationObserver': parameter 1 is not of type 'Node'.").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    };

    // Arg 1: options dict
    let opts_obj = if args.length() > 1 && args.get(1).is_object() {
        Some(unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) })
    } else {
        None
    };

    // Parse options with explicit-vs-default tracking per spec
    let (
        child_list,
        attributes_explicit,
        character_data_explicit,
        subtree,
        attribute_old_value,
        character_data_old_value,
        attribute_filter,
    ) = if let Some(obj) = opts_obj {
        (
            get_bool_opt(scope, obj, "childList").unwrap_or(false),
            get_bool_opt(scope, obj, "attributes"),
            get_bool_opt(scope, obj, "characterData"),
            get_bool_opt(scope, obj, "subtree").unwrap_or(false),
            get_bool_opt(scope, obj, "attributeOldValue").unwrap_or(false),
            get_bool_opt(scope, obj, "characterDataOldValue").unwrap_or(false),
            get_string_array_opt(scope, obj, "attributeFilter"),
        )
    } else {
        (false, None, None, false, false, false, None)
    };

    let mut attributes = attributes_explicit.unwrap_or(false);
    let mut character_data = character_data_explicit.unwrap_or(false);

    // Spec step 1: if attributeOldValue or attributeFilter is set but attributes
    // was not explicitly provided, set attributes = true.
    if (attribute_old_value || attribute_filter.is_some()) && attributes_explicit.is_none() {
        attributes = true;
    }

    // Spec step 2: if characterDataOldValue is set but characterData was not
    // explicitly provided, set characterData = true.
    if character_data_old_value && character_data_explicit.is_none() {
        character_data = true;
    }

    // Spec step 3: at least one of childList/attributes/characterData must be true.
    if !child_list && !attributes && !character_data {
        let msg = v8::String::new(scope, "Failed to execute 'observe' on 'MutationObserver': The options object must set at least one of 'attributes', 'characterData', or 'childList' to true.").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    }

    let options = ObserverOptions {
        child_list,
        attributes,
        character_data,
        subtree,
        attribute_old_value,
        character_data_old_value,
        attribute_filter: attribute_filter.unwrap_or_default(),
    };

    let state = scope.get_slot_mut::<MutationObserverState>().unwrap();
    state.observe(observer_idx, target_id, options);
}

fn mo_disconnect(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let this = args.this();
    let Some(observer_idx) = get_observer_idx(scope, this) else { return };
    let state = scope.get_slot_mut::<MutationObserverState>().unwrap();
    state.disconnect(observer_idx);
}

fn mo_take_records(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    let Some(observer_idx) = get_observer_idx(scope, this) else {
        rv.set(v8::Array::new(scope, 0).into());
        return;
    };
    let arr = crate::js::mutation_observer::build_records_array(scope, observer_idx);
    rv.set(arr.into());
}

// ─── Option parsing helpers ──────────────────────────────────────────────────

fn get_bool_opt(
    scope: &mut v8::HandleScope,
    obj: v8::Local<v8::Object>,
    name: &str,
) -> Option<bool> {
    let k = v8::String::new(scope, name).unwrap();
    let v = obj.get(scope, k.into())?;
    if v.is_undefined() {
        return None;
    }
    Some(v.boolean_value(scope))
}

fn get_string_array_opt(
    scope: &mut v8::HandleScope,
    obj: v8::Local<v8::Object>,
    name: &str,
) -> Option<Vec<String>> {
    let k = v8::String::new(scope, name).unwrap();
    let v = obj.get(scope, k.into())?;
    if v.is_undefined() {
        return None;
    }
    if !v.is_array() {
        return Some(Vec::new());
    }
    let arr = unsafe { v8::Local::<v8::Array>::cast_unchecked(v) };
    let mut result = Vec::new();
    for i in 0..arr.length() {
        if let Some(elem) = arr.get_index(scope, i) {
            result.push(elem.to_rust_string_lossy(scope));
        }
    }
    Some(result)
}

// ─── IntersectionObserver (stub, unchanged) ──────────────────────────────────

fn intersection_observer_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let obj = v8::Object::new(scope);
    let noop = v8::Function::new(scope, |_scope: &mut v8::HandleScope, _args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue| {}).unwrap();
    let take_records = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        rv.set(v8::Array::new(scope, 0).into());
    }).unwrap();
    for name in &["observe", "disconnect", "unobserve"] {
        let k = v8::String::new(scope, name).unwrap();
        obj.set(scope, k.into(), noop.into());
    }
    let k = v8::String::new(scope, "takeRecords").unwrap();
    obj.set(scope, k.into(), take_records.into());

    let k = v8::String::new(scope, "root").unwrap();
    let val = v8::null(scope);
    obj.set(scope, k.into(), val.into());

    let k = v8::String::new(scope, "rootMargin").unwrap();
    let v = v8::String::new(scope, "0px 0px 0px 0px").unwrap();
    obj.set(scope, k.into(), v.into());

    let thresholds = if args.length() > 1 && args.get(1).is_object() {
        let opts = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        let t_key = v8::String::new(scope, "threshold").unwrap();
        if let Some(t_val) = opts.get(scope, t_key.into()) {
            if t_val.is_array() {
                unsafe { v8::Local::<v8::Array>::cast_unchecked(t_val) }
            } else if t_val.is_number() {
                let arr = v8::Array::new(scope, 1);
                arr.set_index(scope, 0, t_val);
                arr
            } else {
                let arr = v8::Array::new(scope, 1);
                let zero = v8::Number::new(scope, 0.0);
                arr.set_index(scope, 0, zero.into());
                arr
            }
        } else {
            let arr = v8::Array::new(scope, 1);
            let zero = v8::Number::new(scope, 0.0);
            arr.set_index(scope, 0, zero.into());
            arr
        }
    } else {
        let arr = v8::Array::new(scope, 1);
        let zero = v8::Number::new(scope, 0.0);
        arr.set_index(scope, 0, zero.into());
        arr
    };
    let k = v8::String::new(scope, "thresholds").unwrap();
    obj.set(scope, k.into(), thresholds.into());

    rv.set(obj.into());
}

// ─── ResizeObserver (stub, unchanged) ────────────────────────────────────────

fn resize_observer_constructor(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let obj = v8::Object::new(scope);
    let noop = v8::Function::new(scope, |_scope: &mut v8::HandleScope, _args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue| {}).unwrap();
    for name in &["observe", "disconnect", "unobserve"] {
        let k = v8::String::new(scope, name).unwrap();
        obj.set(scope, k.into(), noop.into());
    }
    rv.set(obj.into());
}
