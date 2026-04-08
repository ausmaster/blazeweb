/// Event constructor bindings.
///
/// Provides constructors for Event, CustomEvent, MouseEvent, KeyboardEvent,
/// FocusEvent, InputEvent, PointerEvent, ErrorEvent, HashChangeEvent, PopStateEvent.

/// Install all event constructors on the global object.
pub fn install(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    macro_rules! set_ctor {
        ($scope:expr, $global:expr, $name:expr, $cb:ident) => {{
            let func = v8::Function::new($scope, $cb).unwrap();
            let key = v8::String::new($scope, $name).unwrap();
            $global.set($scope, key.into(), func.into());
        }};
    }
    set_ctor!(scope, global, "Event", event_constructor);
    // Enrich Event.prototype with spec methods so polyfills detect native support
    install_event_prototype_methods(scope, global);
    set_ctor!(scope, global, "CustomEvent", custom_event_constructor);
    set_ctor!(scope, global, "MouseEvent", mouse_event_constructor);
    set_ctor!(scope, global, "KeyboardEvent", keyboard_event_constructor);
    set_ctor!(scope, global, "FocusEvent", focus_event_constructor);
    set_ctor!(scope, global, "InputEvent", input_event_constructor);
    set_ctor!(scope, global, "PointerEvent", pointer_event_constructor);
    set_ctor!(scope, global, "ErrorEvent", error_event_constructor);
    set_ctor!(scope, global, "HashChangeEvent", hashchange_event_constructor);
    set_ctor!(scope, global, "PopStateEvent", popstate_event_constructor);
    // Round 2 Phase 3: Additional event constructors
    set_ctor!(scope, global, "UIEvent", uievent_constructor);
    set_ctor!(scope, global, "WheelEvent", wheel_event_constructor);
    set_ctor!(scope, global, "TouchEvent", touch_event_constructor);
    set_ctor!(scope, global, "TransitionEvent", transition_event_constructor);
    set_ctor!(scope, global, "AnimationEvent", animation_event_constructor);
    set_ctor!(scope, global, "MessageEvent", message_event_constructor);
    set_ctor!(scope, global, "CloseEvent", close_event_constructor);
    set_ctor!(scope, global, "ProgressEvent", progress_event_constructor);
    set_ctor!(scope, global, "PromiseRejectionEvent", promise_rejection_event_constructor);
    set_ctor!(scope, global, "SubmitEvent", submit_event_constructor);
    set_ctor!(scope, global, "StorageEvent", storage_event_constructor);
    set_ctor!(scope, global, "ClipboardEvent", clipboard_event_constructor);
    set_ctor!(scope, global, "DragEvent", drag_event_constructor);
    set_ctor!(scope, global, "CompositionEvent", composition_event_constructor);
    set_ctor!(scope, global, "SecurityPolicyViolationEvent", secpolicy_event_constructor);
    log::debug!("Installed 16 additional event constructors (Round 2)");
}

fn event_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = v8::Object::new(scope);

    let set_str = |scope: &mut v8::HandleScope, obj: v8::Local<v8::Object>, key: &str, val: &str| {
        let k = v8::String::new(scope, key).unwrap();
        let v = v8::String::new(scope, val).unwrap();
        obj.set(scope, k.into(), v.into());
    };
    let set_bool = |scope: &mut v8::HandleScope, obj: v8::Local<v8::Object>, key: &str, val: bool| {
        let k = v8::String::new(scope, key).unwrap();
        let v = v8::Boolean::new(scope, val);
        obj.set(scope, k.into(), v.into());
    };

    set_str(scope, obj, "type", &event_type);

    let (bubbles, cancelable, composed) = if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        let b_key = v8::String::new(scope, "bubbles").unwrap();
        let c_key = v8::String::new(scope, "cancelable").unwrap();
        let comp_key = v8::String::new(scope, "composed").unwrap();
        let bubbles = init.get(scope, b_key.into()).map(|v| v.boolean_value(scope)).unwrap_or(false);
        let cancelable = init.get(scope, c_key.into()).map(|v| v.boolean_value(scope)).unwrap_or(false);
        let composed = init.get(scope, comp_key.into()).map(|v| v.boolean_value(scope)).unwrap_or(false);
        (bubbles, cancelable, composed)
    } else {
        (false, false, false)
    };

    set_bool(scope, obj, "bubbles", bubbles);
    set_bool(scope, obj, "cancelable", cancelable);
    set_bool(scope, obj, "composed", composed);
    set_bool(scope, obj, "defaultPrevented", false);
    set_bool(scope, obj, "isTrusted", false);

    let null = v8::null(scope);
    let k = v8::String::new(scope, "target").unwrap();
    obj.set(scope, k.into(), null.into());
    let k = v8::String::new(scope, "currentTarget").unwrap();
    obj.set(scope, k.into(), null.into());

    let prevent = v8::Function::new(scope, |scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue| {
        let this = args.this();
        let ck = v8::String::new(scope, "cancelable").unwrap();
        let cancelable = this.get(scope, ck.into())
            .map(|v| v.boolean_value(scope))
            .unwrap_or(false);
        if cancelable {
            let k = v8::String::new(scope, "defaultPrevented").unwrap();
            let v = v8::Boolean::new(scope, true);
            this.set(scope, k.into(), v.into());
        }
    }).unwrap();
    let k = v8::String::new(scope, "preventDefault").unwrap();
    obj.set(scope, k.into(), prevent.into());

    crate::js::events::install_propagation_flags(scope, obj);

    rv.set(obj.into());
}

fn custom_event_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = v8::Object::new(scope);

    let set_str = |scope: &mut v8::HandleScope, obj: v8::Local<v8::Object>, key: &str, val: &str| {
        let k = v8::String::new(scope, key).unwrap();
        let v = v8::String::new(scope, val).unwrap();
        obj.set(scope, k.into(), v.into());
    };
    let set_bool = |scope: &mut v8::HandleScope, obj: v8::Local<v8::Object>, key: &str, val: bool| {
        let k = v8::String::new(scope, key).unwrap();
        let v = v8::Boolean::new(scope, val);
        obj.set(scope, k.into(), v.into());
    };

    set_str(scope, obj, "type", &event_type);

    let (bubbles, cancelable, detail) = if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        let b_key = v8::String::new(scope, "bubbles").unwrap();
        let c_key = v8::String::new(scope, "cancelable").unwrap();
        let d_key = v8::String::new(scope, "detail").unwrap();
        let bubbles = init.get(scope, b_key.into()).map(|v| v.boolean_value(scope)).unwrap_or(false);
        let cancelable = init.get(scope, c_key.into()).map(|v| v.boolean_value(scope)).unwrap_or(false);
        let detail: Option<v8::Local<v8::Value>> = init.get(scope, d_key.into());
        (bubbles, cancelable, detail)
    } else {
        (false, false, None)
    };

    set_bool(scope, obj, "bubbles", bubbles);
    set_bool(scope, obj, "cancelable", cancelable);
    set_bool(scope, obj, "defaultPrevented", false);
    set_bool(scope, obj, "isTrusted", false);

    let null_val = v8::null(scope);
    let k = v8::String::new(scope, "target").unwrap();
    obj.set(scope, k.into(), null_val.into());
    let k = v8::String::new(scope, "currentTarget").unwrap();
    obj.set(scope, k.into(), null_val.into());

    let d_key = v8::String::new(scope, "detail").unwrap();
    if let Some(d) = detail {
        obj.set(scope, d_key.into(), d);
    } else {
        let null2 = v8::null(scope);
        obj.set(scope, d_key.into(), null2.into());
    }

    let prevent = v8::Function::new(scope, |scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue| {
        let this = args.this();
        let ck = v8::String::new(scope, "cancelable").unwrap();
        let cancelable = this.get(scope, ck.into())
            .map(|v| v.boolean_value(scope))
            .unwrap_or(false);
        if cancelable {
            let k = v8::String::new(scope, "defaultPrevented").unwrap();
            let v = v8::Boolean::new(scope, true);
            this.set(scope, k.into(), v.into());
        }
    }).unwrap();
    let k = v8::String::new(scope, "preventDefault").unwrap();
    obj.set(scope, k.into(), prevent.into());

    crate::js::events::install_propagation_flags(scope, obj);

    rv.set(obj.into());
}

pub fn build_base_event<'s>(
    scope: &mut v8::HandleScope<'s>,
    event_type: &str,
    args: &v8::FunctionCallbackArguments,
) -> v8::Local<'s, v8::Object> {
    let obj = v8::Object::new(scope);
    let k = v8::String::new(scope, "type").unwrap();
    let v = v8::String::new(scope, event_type).unwrap();
    obj.set(scope, k.into(), v.into());

    let (bubbles, cancelable, composed) = if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        let b_key = v8::String::new(scope, "bubbles").unwrap();
        let c_key = v8::String::new(scope, "cancelable").unwrap();
        let composed_key = v8::String::new(scope, "composed").unwrap();
        let b = init.get(scope, b_key.into()).map(|v| v.boolean_value(scope)).unwrap_or(false);
        let c = init.get(scope, c_key.into()).map(|v| v.boolean_value(scope)).unwrap_or(false);
        let comp = init.get(scope, composed_key.into()).map(|v| v.boolean_value(scope)).unwrap_or(false);
        (b, c, comp)
    } else {
        (false, false, false)
    };

    let k = v8::String::new(scope, "bubbles").unwrap();
    let val = v8::Boolean::new(scope, bubbles);
    obj.set(scope, k.into(), val.into());
    let k = v8::String::new(scope, "cancelable").unwrap();
    let val = v8::Boolean::new(scope, cancelable);
    obj.set(scope, k.into(), val.into());
    let k = v8::String::new(scope, "composed").unwrap();
    let val = v8::Boolean::new(scope, composed);
    obj.set(scope, k.into(), val.into());
    let k = v8::String::new(scope, "defaultPrevented").unwrap();
    let val = v8::Boolean::new(scope, false);
    obj.set(scope, k.into(), val.into());
    let k = v8::String::new(scope, "isTrusted").unwrap();
    let val = v8::Boolean::new(scope, false);
    obj.set(scope, k.into(), val.into());
    let null = v8::null(scope);
    let k = v8::String::new(scope, "target").unwrap();
    obj.set(scope, k.into(), null.into());
    let k = v8::String::new(scope, "currentTarget").unwrap();
    obj.set(scope, k.into(), null.into());

    let prevent = v8::Function::new(scope, |scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _: v8::ReturnValue| {
        let this = args.this();
        let ck = v8::String::new(scope, "cancelable").unwrap();
        let cancelable = this.get(scope, ck.into())
            .map(|v| v.boolean_value(scope))
            .unwrap_or(false);
        if cancelable {
            let k = v8::String::new(scope, "defaultPrevented").unwrap();
            let val = v8::Boolean::new(scope, true);
            this.set(scope, k.into(), val.into());
        }
    }).unwrap();
    let k = v8::String::new(scope, "preventDefault").unwrap();
    obj.set(scope, k.into(), prevent.into());
    crate::js::events::install_propagation_flags(scope, obj);

    obj
}

fn read_init_number(scope: &mut v8::HandleScope, init: v8::Local<v8::Object>, prop: &str) -> f64 {
    let k = v8::String::new(scope, prop).unwrap();
    init.get(scope, k.into())
        .and_then(|v| v.number_value(scope))
        .unwrap_or(0.0)
}

fn read_init_bool(scope: &mut v8::HandleScope, init: v8::Local<v8::Object>, prop: &str) -> bool {
    let k = v8::String::new(scope, prop).unwrap();
    init.get(scope, k.into())
        .map(|v| v.boolean_value(scope))
        .unwrap_or(false)
}

fn read_init_string(scope: &mut v8::HandleScope, init: v8::Local<v8::Object>, prop: &str) -> String {
    let k = v8::String::new(scope, prop).unwrap();
    init.get(scope, k.into())
        .map(|v| v.to_rust_string_lossy(scope))
        .unwrap_or_default()
}

fn mouse_event_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    let zero = v8::Number::new(scope, 0.0);
    let izero = v8::Integer::new(scope, 0);
    let f = v8::Boolean::new(scope, false);

    if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        for prop in &["clientX", "clientY", "pageX", "pageY", "screenX", "screenY", "offsetX", "offsetY", "movementX", "movementY"] {
            let val = read_init_number(scope, init, prop);
            let k = v8::String::new(scope, prop).unwrap();
            let v = v8::Number::new(scope, val);
            obj.set(scope, k.into(), v.into());
        }
        for prop in &["button", "buttons", "detail"] {
            let val = read_init_number(scope, init, prop);
            let k = v8::String::new(scope, prop).unwrap();
            let v = v8::Integer::new(scope, val as i32);
            obj.set(scope, k.into(), v.into());
        }
        for prop in &["altKey", "ctrlKey", "metaKey", "shiftKey"] {
            let val = read_init_bool(scope, init, prop);
            let k = v8::String::new(scope, prop).unwrap();
            let v = v8::Boolean::new(scope, val);
            obj.set(scope, k.into(), v.into());
        }
        let rk = v8::String::new(scope, "relatedTarget").unwrap();
        let rt = init.get(scope, rk.into()).unwrap_or_else(|| v8::null(scope).into());
        obj.set(scope, rk.into(), rt);
    } else {
        for prop in &["clientX", "clientY", "pageX", "pageY", "screenX", "screenY", "offsetX", "offsetY", "movementX", "movementY"] {
            let k = v8::String::new(scope, prop).unwrap();
            obj.set(scope, k.into(), zero.into());
        }
        for prop in &["button", "buttons", "detail"] {
            let k = v8::String::new(scope, prop).unwrap();
            obj.set(scope, k.into(), izero.into());
        }
        for prop in &["altKey", "ctrlKey", "metaKey", "shiftKey"] {
            let k = v8::String::new(scope, prop).unwrap();
            obj.set(scope, k.into(), f.into());
        }
        let k = v8::String::new(scope, "relatedTarget").unwrap();
        let val = v8::null(scope);
        obj.set(scope, k.into(), val.into());
    }
    let gms = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        rv.set(v8::Boolean::new(scope, false).into());
    }).unwrap();
    let k = v8::String::new(scope, "getModifierState").unwrap();
    obj.set(scope, k.into(), gms.into());
    rv.set(obj.into());
}

fn keyboard_event_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);

    if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        for prop in &["key", "code"] {
            let val = read_init_string(scope, init, prop);
            let k = v8::String::new(scope, prop).unwrap();
            let v = v8::String::new(scope, &val).unwrap();
            obj.set(scope, k.into(), v.into());
        }
        for prop in &["keyCode", "charCode", "which", "location"] {
            let val = read_init_number(scope, init, prop);
            let k = v8::String::new(scope, prop).unwrap();
            let v = v8::Integer::new(scope, val as i32);
            obj.set(scope, k.into(), v.into());
        }
        for prop in &["altKey", "ctrlKey", "metaKey", "shiftKey", "repeat", "isComposing"] {
            let val = read_init_bool(scope, init, prop);
            let k = v8::String::new(scope, prop).unwrap();
            let v = v8::Boolean::new(scope, val);
            obj.set(scope, k.into(), v.into());
        }
    } else {
        let empty = v8::String::new(scope, "").unwrap();
        for prop in &["key", "code"] {
            let k = v8::String::new(scope, prop).unwrap();
            obj.set(scope, k.into(), empty.into());
        }
        let izero = v8::Integer::new(scope, 0);
        for prop in &["keyCode", "charCode", "which", "location"] {
            let k = v8::String::new(scope, prop).unwrap();
            obj.set(scope, k.into(), izero.into());
        }
        let f = v8::Boolean::new(scope, false);
        for prop in &["altKey", "ctrlKey", "metaKey", "shiftKey", "repeat", "isComposing"] {
            let k = v8::String::new(scope, prop).unwrap();
            obj.set(scope, k.into(), f.into());
        }
    }
    let gms = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        rv.set(v8::Boolean::new(scope, false).into());
    }).unwrap();
    let k = v8::String::new(scope, "getModifierState").unwrap();
    obj.set(scope, k.into(), gms.into());
    rv.set(obj.into());
}

fn focus_event_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    let k = v8::String::new(scope, "relatedTarget").unwrap();
    if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        let rt = init.get(scope, k.into()).unwrap_or_else(|| v8::null(scope).into());
        obj.set(scope, k.into(), rt);
    } else {
        let val = v8::null(scope);
        obj.set(scope, k.into(), val.into());
    }
    rv.set(obj.into());
}

fn input_event_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        let data = read_init_string(scope, init, "data");
        let k = v8::String::new(scope, "data").unwrap();
        let v = v8::String::new(scope, &data).unwrap();
        obj.set(scope, k.into(), v.into());
        let it = read_init_string(scope, init, "inputType");
        let k = v8::String::new(scope, "inputType").unwrap();
        let v = v8::String::new(scope, &it).unwrap();
        obj.set(scope, k.into(), v.into());
        let ic = read_init_bool(scope, init, "isComposing");
        let k = v8::String::new(scope, "isComposing").unwrap();
        let val = v8::Boolean::new(scope, ic);
        obj.set(scope, k.into(), val.into());
    } else {
        let k = v8::String::new(scope, "data").unwrap();
        let val = v8::null(scope);
        obj.set(scope, k.into(), val.into());
        let k = v8::String::new(scope, "inputType").unwrap();
        let v = v8::String::new(scope, "").unwrap();
        obj.set(scope, k.into(), v.into());
        let k = v8::String::new(scope, "isComposing").unwrap();
        let val = v8::Boolean::new(scope, false);
        obj.set(scope, k.into(), val.into());
    }
    rv.set(obj.into());
}

fn pointer_event_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    let zero = v8::Number::new(scope, 0.0);
    let izero = v8::Integer::new(scope, 0);
    for prop in &["clientX", "clientY", "pageX", "pageY", "screenX", "screenY", "offsetX", "offsetY"] {
        let k = v8::String::new(scope, prop).unwrap();
        obj.set(scope, k.into(), zero.into());
    }
    for prop in &["button", "buttons"] {
        let k = v8::String::new(scope, prop).unwrap();
        obj.set(scope, k.into(), izero.into());
    }
    if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        let pid = read_init_number(scope, init, "pointerId");
        let k = v8::String::new(scope, "pointerId").unwrap();
        let val = v8::Integer::new(scope, pid as i32);
        obj.set(scope, k.into(), val.into());
        let pt = read_init_string(scope, init, "pointerType");
        let k = v8::String::new(scope, "pointerType").unwrap();
        let v = v8::String::new(scope, &pt).unwrap();
        obj.set(scope, k.into(), v.into());
        let is_primary = read_init_bool(scope, init, "isPrimary");
        let k = v8::String::new(scope, "isPrimary").unwrap();
        let val = v8::Boolean::new(scope, is_primary);
        obj.set(scope, k.into(), val.into());
        let w = read_init_number(scope, init, "width");
        let k = v8::String::new(scope, "width").unwrap();
        let val = v8::Number::new(scope, if w == 0.0 { 1.0 } else { w });
        obj.set(scope, k.into(), val.into());
        let h = read_init_number(scope, init, "height");
        let k = v8::String::new(scope, "height").unwrap();
        let val = v8::Number::new(scope, if h == 0.0 { 1.0 } else { h });
        obj.set(scope, k.into(), val.into());
        let p = read_init_number(scope, init, "pressure");
        let k = v8::String::new(scope, "pressure").unwrap();
        let val = v8::Number::new(scope, p);
        obj.set(scope, k.into(), val.into());
    } else {
        let k = v8::String::new(scope, "pointerId").unwrap();
        obj.set(scope, k.into(), izero.into());
        let k = v8::String::new(scope, "pointerType").unwrap();
        let v = v8::String::new(scope, "").unwrap();
        obj.set(scope, k.into(), v.into());
        let k = v8::String::new(scope, "isPrimary").unwrap();
        let val = v8::Boolean::new(scope, false);
        obj.set(scope, k.into(), val.into());
        let one = v8::Number::new(scope, 1.0);
        let k = v8::String::new(scope, "width").unwrap();
        obj.set(scope, k.into(), one.into());
        let k = v8::String::new(scope, "height").unwrap();
        obj.set(scope, k.into(), one.into());
        let k = v8::String::new(scope, "pressure").unwrap();
        obj.set(scope, k.into(), zero.into());
    }
    let gms = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        rv.set(v8::Boolean::new(scope, false).into());
    }).unwrap();
    let k = v8::String::new(scope, "getModifierState").unwrap();
    obj.set(scope, k.into(), gms.into());
    rv.set(obj.into());
}

fn error_event_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    let empty = v8::String::new(scope, "").unwrap();
    let izero = v8::Integer::new(scope, 0);
    if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        let msg = read_init_string(scope, init, "message");
        let k = v8::String::new(scope, "message").unwrap();
        let v = v8::String::new(scope, &msg).unwrap();
        obj.set(scope, k.into(), v.into());
        let filename = read_init_string(scope, init, "filename");
        let k = v8::String::new(scope, "filename").unwrap();
        let v = v8::String::new(scope, &filename).unwrap();
        obj.set(scope, k.into(), v.into());
        let lineno = read_init_number(scope, init, "lineno");
        let k = v8::String::new(scope, "lineno").unwrap();
        let val = v8::Integer::new(scope, lineno as i32);
        obj.set(scope, k.into(), val.into());
        let colno = read_init_number(scope, init, "colno");
        let k = v8::String::new(scope, "colno").unwrap();
        let val = v8::Integer::new(scope, colno as i32);
        obj.set(scope, k.into(), val.into());
        let err_key = v8::String::new(scope, "error").unwrap();
        let err = init.get(scope, err_key.into()).unwrap_or_else(|| v8::null(scope).into());
        obj.set(scope, err_key.into(), err);
    } else {
        let k = v8::String::new(scope, "message").unwrap();
        obj.set(scope, k.into(), empty.into());
        let k = v8::String::new(scope, "filename").unwrap();
        obj.set(scope, k.into(), empty.into());
        let k = v8::String::new(scope, "lineno").unwrap();
        obj.set(scope, k.into(), izero.into());
        let k = v8::String::new(scope, "colno").unwrap();
        obj.set(scope, k.into(), izero.into());
        let k = v8::String::new(scope, "error").unwrap();
        let val = v8::null(scope);
        obj.set(scope, k.into(), val.into());
    }
    rv.set(obj.into());
}

fn hashchange_event_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    let empty = v8::String::new(scope, "").unwrap();
    if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        for prop in &["oldURL", "newURL"] {
            let val = read_init_string(scope, init, prop);
            let k = v8::String::new(scope, prop).unwrap();
            let v = v8::String::new(scope, &val).unwrap();
            obj.set(scope, k.into(), v.into());
        }
    } else {
        for prop in &["oldURL", "newURL"] {
            let k = v8::String::new(scope, prop).unwrap();
            obj.set(scope, k.into(), empty.into());
        }
    }
    rv.set(obj.into());
}

fn popstate_event_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    let k = v8::String::new(scope, "state").unwrap();
    if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        let state = init.get(scope, k.into()).unwrap_or_else(|| v8::null(scope).into());
        obj.set(scope, k.into(), state);
    } else {
        let val = v8::null(scope);
        obj.set(scope, k.into(), val.into());
    }
    rv.set(obj.into());
}

// ─── Round 2 Phase 3: Additional event constructors ─────────────────────────

/// Helper: set a string property on an object.
fn set_s(scope: &mut v8::HandleScope, obj: v8::Local<v8::Object>, key: &str, val: &str) {
    let k = v8::String::new(scope, key).unwrap();
    let v = v8::String::new(scope, val).unwrap();
    obj.set(scope, k.into(), v.into());
}

/// Helper: set a number property on an object.
fn set_n(scope: &mut v8::HandleScope, obj: v8::Local<v8::Object>, key: &str, val: f64) {
    let k = v8::String::new(scope, key).unwrap();
    let v = v8::Number::new(scope, val);
    obj.set(scope, k.into(), v.into());
}

/// Helper: set an int property on an object.
fn set_i(scope: &mut v8::HandleScope, obj: v8::Local<v8::Object>, key: &str, val: i32) {
    let k = v8::String::new(scope, key).unwrap();
    let v = v8::Integer::new(scope, val);
    obj.set(scope, k.into(), v.into());
}

/// Helper: set a bool property on an object.
fn set_b(scope: &mut v8::HandleScope, obj: v8::Local<v8::Object>, key: &str, val: bool) {
    let k = v8::String::new(scope, key).unwrap();
    let v = v8::Boolean::new(scope, val);
    obj.set(scope, k.into(), v.into());
}

/// Helper: extract a string from init dict.
fn get_str(scope: &mut v8::HandleScope, init: v8::Local<v8::Object>, key: &str) -> String {
    let k = v8::String::new(scope, key).unwrap();
    init.get(scope, k.into())
        .filter(|v| !v.is_undefined())
        .map(|v| v.to_rust_string_lossy(scope))
        .unwrap_or_default()
}

/// Helper: extract a number from init dict.
fn get_num(scope: &mut v8::HandleScope, init: v8::Local<v8::Object>, key: &str, default: f64) -> f64 {
    let k = v8::String::new(scope, key).unwrap();
    init.get(scope, k.into())
        .filter(|v| !v.is_undefined())
        .map(|v| v.number_value(scope).unwrap_or(default))
        .unwrap_or(default)
}

/// Helper: extract an int from init dict.
fn get_int(scope: &mut v8::HandleScope, init: v8::Local<v8::Object>, key: &str, default: i32) -> i32 {
    let k = v8::String::new(scope, key).unwrap();
    init.get(scope, k.into())
        .filter(|v| !v.is_undefined())
        .map(|v| v.int32_value(scope).unwrap_or(default))
        .unwrap_or(default)
}

/// Helper: extract a bool from init dict.
fn get_bool(scope: &mut v8::HandleScope, init: v8::Local<v8::Object>, key: &str, default: bool) -> bool {
    let k = v8::String::new(scope, key).unwrap();
    init.get(scope, k.into())
        .filter(|v| !v.is_undefined())
        .map(|v| v.boolean_value(scope))
        .unwrap_or(default)
}

fn uievent_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    let detail = if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        get_int(scope, init, "detail", 0)
    } else { 0 };
    set_i(scope, obj, "detail", detail);
    let null = v8::null(scope);
    let k = v8::String::new(scope, "view").unwrap();
    obj.set(scope, k.into(), null.into());
    rv.set(obj.into());
}

fn wheel_event_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    let (dx, dy, dz, dm) = if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        (get_num(scope, init, "deltaX", 0.0), get_num(scope, init, "deltaY", 0.0),
         get_num(scope, init, "deltaZ", 0.0), get_int(scope, init, "deltaMode", 0))
    } else { (0.0, 0.0, 0.0, 0) };
    set_n(scope, obj, "deltaX", dx);
    set_n(scope, obj, "deltaY", dy);
    set_n(scope, obj, "deltaZ", dz);
    set_i(scope, obj, "deltaMode", dm);
    rv.set(obj.into());
}

fn touch_event_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    for name in &["touches", "targetTouches", "changedTouches"] {
        let empty = v8::Array::new(scope, 0);
        let k = v8::String::new(scope, name).unwrap();
        obj.set(scope, k.into(), empty.into());
    }
    rv.set(obj.into());
}

fn transition_event_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    let (pn, et, pe) = if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        (get_str(scope, init, "propertyName"), get_num(scope, init, "elapsedTime", 0.0), get_str(scope, init, "pseudoElement"))
    } else { (String::new(), 0.0, String::new()) };
    set_s(scope, obj, "propertyName", &pn);
    set_n(scope, obj, "elapsedTime", et);
    set_s(scope, obj, "pseudoElement", &pe);
    rv.set(obj.into());
}

fn animation_event_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    let (an, et, pe) = if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        (get_str(scope, init, "animationName"), get_num(scope, init, "elapsedTime", 0.0), get_str(scope, init, "pseudoElement"))
    } else { (String::new(), 0.0, String::new()) };
    set_s(scope, obj, "animationName", &an);
    set_n(scope, obj, "elapsedTime", et);
    set_s(scope, obj, "pseudoElement", &pe);
    rv.set(obj.into());
}

fn message_event_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    // Extract data from init dict
    let k = v8::String::new(scope, "data").unwrap();
    let data = if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        init.get(scope, k.into()).filter(|v| !v.is_undefined()).unwrap_or_else(|| v8::null(scope).into())
    } else { v8::null(scope).into() };
    obj.set(scope, k.into(), data);
    let (origin, last_id) = if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        (get_str(scope, init, "origin"), get_str(scope, init, "lastEventId"))
    } else { (String::new(), String::new()) };
    set_s(scope, obj, "origin", &origin);
    set_s(scope, obj, "lastEventId", &last_id);
    let null = v8::null(scope);
    let k = v8::String::new(scope, "source").unwrap();
    obj.set(scope, k.into(), null.into());
    let empty = v8::Array::new(scope, 0);
    let k = v8::String::new(scope, "ports").unwrap();
    obj.set(scope, k.into(), empty.into());
    rv.set(obj.into());
}

fn close_event_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    let (wc, code, reason) = if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        (get_bool(scope, init, "wasClean", false), get_int(scope, init, "code", 0), get_str(scope, init, "reason"))
    } else { (false, 0, String::new()) };
    set_b(scope, obj, "wasClean", wc);
    set_i(scope, obj, "code", code);
    set_s(scope, obj, "reason", &reason);
    rv.set(obj.into());
}

fn progress_event_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    let (lc, loaded, total) = if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        (get_bool(scope, init, "lengthComputable", false), get_num(scope, init, "loaded", 0.0), get_num(scope, init, "total", 0.0))
    } else { (false, 0.0, 0.0) };
    set_b(scope, obj, "lengthComputable", lc);
    set_n(scope, obj, "loaded", loaded);
    set_n(scope, obj, "total", total);
    rv.set(obj.into());
}

fn promise_rejection_event_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        let k = v8::String::new(scope, "promise").unwrap();
        let v = init.get(scope, k.into()).filter(|v| !v.is_undefined()).unwrap_or_else(|| v8::undefined(scope).into());
        obj.set(scope, k.into(), v);
        let k = v8::String::new(scope, "reason").unwrap();
        let v = init.get(scope, k.into()).filter(|v| !v.is_undefined()).unwrap_or_else(|| v8::undefined(scope).into());
        obj.set(scope, k.into(), v);
    } else {
        let undef = v8::undefined(scope);
        let k = v8::String::new(scope, "promise").unwrap();
        obj.set(scope, k.into(), undef.into());
        let k = v8::String::new(scope, "reason").unwrap();
        obj.set(scope, k.into(), undef.into());
    }
    rv.set(obj.into());
}

fn submit_event_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    let k = v8::String::new(scope, "submitter").unwrap();
    if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        let v = init.get(scope, k.into()).filter(|v| !v.is_undefined()).unwrap_or_else(|| v8::null(scope).into());
        obj.set(scope, k.into(), v);
    } else {
        let null = v8::null(scope);
        obj.set(scope, k.into(), null.into());
    }
    rv.set(obj.into());
}

fn storage_event_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    let null = v8::null(scope);
    if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        for key in &["key", "oldValue", "newValue", "url"] {
            let k = v8::String::new(scope, key).unwrap();
            let val = init.get(scope, k.into()).filter(|v| !v.is_undefined()).unwrap_or_else(|| null.into());
            obj.set(scope, k.into(), val);
        }
    } else {
        for key in &["key", "oldValue", "newValue", "url"] {
            let k = v8::String::new(scope, key).unwrap();
            obj.set(scope, k.into(), null.into());
        }
    }
    let k = v8::String::new(scope, "storageArea").unwrap();
    obj.set(scope, k.into(), null.into());
    rv.set(obj.into());
}

fn clipboard_event_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    let null = v8::null(scope);
    let k = v8::String::new(scope, "clipboardData").unwrap();
    obj.set(scope, k.into(), null.into());
    rv.set(obj.into());
}

fn drag_event_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    let null = v8::null(scope);
    let k = v8::String::new(scope, "dataTransfer").unwrap();
    obj.set(scope, k.into(), null.into());
    rv.set(obj.into());
}

fn composition_event_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    let data = if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        get_str(scope, init, "data")
    } else { String::new() };
    set_s(scope, obj, "data", &data);
    rv.set(obj.into());
}

fn secpolicy_event_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let obj = build_base_event(scope, &event_type, &args);
    set_s(scope, obj, "documentURI", "");
    set_s(scope, obj, "referrer", "");
    set_s(scope, obj, "violatedDirective", "");
    set_s(scope, obj, "effectiveDirective", "");
    set_s(scope, obj, "originalPolicy", "");
    set_s(scope, obj, "disposition", "enforce");
    set_s(scope, obj, "blockedURI", "");
    set_i(scope, obj, "statusCode", 0);
    rv.set(obj.into());
}

// ─── EventTarget constructor ────────────────────────────────────────────────

/// Install EventTarget as a proper JS class using V8's script evaluation.
/// This gives us correct `extends EventTarget`, `instanceof`, and prototype chain
/// semantics that are impossible to achieve with v8::Function::new alone (since
/// FunctionTemplate requires a no-context scope).
/// Install standard methods on Event.prototype so polyfills (webcomponents-sd.js,
/// ShadyDOM) detect native support via `Event.prototype.composedPath` etc.
fn install_event_prototype_methods(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    let source = r#"
    (function(g) {
        var EP = g.Event.prototype;
        // composedPath — returns the event path (set per-instance during dispatch,
        // but must exist on prototype for polyfill detection)
        if (!EP.composedPath) {
            EP.composedPath = function() {
                return this.__composedPath || [];
            };
        }
        // composed — defaults to false (per-instance override during construction)
        if (!("composed" in EP)) {
            Object.defineProperty(EP, "composed", {
                get: function() { return this._composed || false; },
                configurable: true, enumerable: true
            });
        }
        // preventDefault
        if (!EP.preventDefault) {
            EP.preventDefault = function() {
                if (this.cancelable) {
                    Object.defineProperty(this, "defaultPrevented", {
                        get: function() { return true; }, configurable: true
                    });
                }
            };
        }
        // stopPropagation / stopImmediatePropagation
        if (!EP.stopPropagation) {
            EP.stopPropagation = function() { this.__stopProp = true; };
        }
        if (!EP.stopImmediatePropagation) {
            EP.stopImmediatePropagation = function() { this.__stopProp = true; this.__stopImm = true; };
        }
        // initEvent (legacy DOM Level 2)
        if (!EP.initEvent) {
            EP.initEvent = function(type, bubbles, cancelable) {
                this.type = type;
                this.bubbles = !!bubbles;
                this.cancelable = !!cancelable;
            };
        }
    })(self)
    "#;
    super::window::run_js(scope, source, "[blazeweb:Event.prototype]");
    let _ = global;
    log::debug!("Installed Event.prototype methods (composedPath, composed, preventDefault, stopPropagation, initEvent)");
}

pub fn install_event_target(scope: &mut v8::HandleScope, _global: v8::Local<v8::Object>) {
    let source = r#"
    (function() {
        "use strict";
        var _sym = Symbol("__et_listeners");
        class EventTarget {
            constructor() {
                Object.defineProperty(this, _sym, {value: Object.create(null), enumerable: false});
            }
            addEventListener(type, cb, options) {
                if (typeof cb !== "function" && typeof cb !== "object") return;
                var listeners = this[_sym];
                if (!listeners) return;
                if (!listeners[type]) listeners[type] = [];
                var arr = listeners[type];
                // Deduplicate
                for (var i = 0; i < arr.length; i++) {
                    if (arr[i] === cb) return;
                }
                arr.push(cb);
            }
            removeEventListener(type, cb) {
                var listeners = this[_sym];
                if (!listeners || !listeners[type]) return;
                var arr = listeners[type];
                for (var i = 0; i < arr.length; i++) {
                    if (arr[i] === cb) { arr.splice(i, 1); return; }
                }
            }
            dispatchEvent(event) {
                if (!event || typeof event.type !== "string") return false;
                var listeners = this[_sym];
                event.target = this;
                event.currentTarget = this;
                if (listeners && listeners[event.type]) {
                    var arr = listeners[event.type].slice();
                    for (var i = 0; i < arr.length; i++) {
                        if (typeof arr[i] === "function") arr[i].call(this, event);
                    }
                }
                return !event.defaultPrevented;
            }
        }
        return EventTarget;
    })()
    "#;
    let source_str = v8::String::new(scope, source).unwrap();
    let name = v8::String::new(scope, "[blazeweb:EventTarget]").unwrap();
    let origin = v8::ScriptOrigin::new(
        scope,
        name.into(),
        0, 0, false, -1,
        None, false, false, false, None,
    );
    let script = v8::Script::compile(scope, source_str, Some(&origin));
    if let Some(script) = script {
        if let Some(result) = script.run(scope) {
            let key = v8::String::new(scope, "EventTarget").unwrap();
            let global = scope.get_current_context().global(scope);
            global.set(scope, key.into(), result);
            log::debug!("Installed EventTarget class (JS-defined, supports extends/instanceof)");
        }
    }
}
