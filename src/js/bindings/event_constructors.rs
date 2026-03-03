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
    set_ctor!(scope, global, "CustomEvent", custom_event_constructor);
    set_ctor!(scope, global, "MouseEvent", mouse_event_constructor);
    set_ctor!(scope, global, "KeyboardEvent", keyboard_event_constructor);
    set_ctor!(scope, global, "FocusEvent", focus_event_constructor);
    set_ctor!(scope, global, "InputEvent", input_event_constructor);
    set_ctor!(scope, global, "PointerEvent", pointer_event_constructor);
    set_ctor!(scope, global, "ErrorEvent", error_event_constructor);
    set_ctor!(scope, global, "HashChangeEvent", hashchange_event_constructor);
    set_ctor!(scope, global, "PopStateEvent", popstate_event_constructor);
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

    let (bubbles, cancelable) = if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        let b_key = v8::String::new(scope, "bubbles").unwrap();
        let c_key = v8::String::new(scope, "cancelable").unwrap();
        let bubbles = init.get(scope, b_key.into()).map(|v| v.boolean_value(scope)).unwrap_or(false);
        let cancelable = init.get(scope, c_key.into()).map(|v| v.boolean_value(scope)).unwrap_or(false);
        (bubbles, cancelable)
    } else {
        (false, false)
    };

    set_bool(scope, obj, "bubbles", bubbles);
    set_bool(scope, obj, "cancelable", cancelable);
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

    let (bubbles, cancelable) = if args.length() > 1 && args.get(1).is_object() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        let b_key = v8::String::new(scope, "bubbles").unwrap();
        let c_key = v8::String::new(scope, "cancelable").unwrap();
        let b = init.get(scope, b_key.into()).map(|v| v.boolean_value(scope)).unwrap_or(false);
        let c = init.get(scope, c_key.into()).map(|v| v.boolean_value(scope)).unwrap_or(false);
        (b, c)
    } else {
        (false, false)
    };

    let k = v8::String::new(scope, "bubbles").unwrap();
    let val = v8::Boolean::new(scope, bubbles);
    obj.set(scope, k.into(), val.into());
    let k = v8::String::new(scope, "cancelable").unwrap();
    let val = v8::Boolean::new(scope, cancelable);
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
