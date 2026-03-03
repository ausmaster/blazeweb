/// FormData API — key/value pair store for form data.

/// Install the FormData constructor on the global object.
pub fn install(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    let formdata_ctor = v8::Function::new(scope, formdata_constructor).unwrap();
    let key = v8::String::new(scope, "FormData").unwrap();
    global.set(scope, key.into(), formdata_ctor.into());
}

fn formdata_constructor(scope: &mut v8::HandleScope, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let obj = v8::Object::new(scope);
    let pairs = v8::Array::new(scope, 0);
    let pk = v8::String::new(scope, "__pairs").unwrap();
    let hidden_key = v8::Private::for_api(scope, Some(pk));
    obj.set_private(scope, hidden_key, pairs.into());

    macro_rules! set_fd_method {
        ($scope:expr, $obj:expr, $name:expr, $cb:ident) => {{
            let func = v8::Function::new($scope, $cb).unwrap();
            let k = v8::String::new($scope, $name).unwrap();
            $obj.set($scope, k.into(), func.into());
        }};
    }
    set_fd_method!(scope, obj, "append", fd_append);
    set_fd_method!(scope, obj, "get", fd_get);
    set_fd_method!(scope, obj, "getAll", fd_get_all);
    set_fd_method!(scope, obj, "set", fd_set);
    set_fd_method!(scope, obj, "has", fd_has);
    set_fd_method!(scope, obj, "delete", fd_delete);
    set_fd_method!(scope, obj, "entries", fd_entries);
    set_fd_method!(scope, obj, "keys", fd_keys);
    set_fd_method!(scope, obj, "values", fd_values);
    set_fd_method!(scope, obj, "forEach", fd_for_each);
    rv.set(obj.into());
}

pub fn fd_get_pairs<'s>(scope: &mut v8::HandleScope<'s>, this: v8::Local<v8::Object>) -> Option<v8::Local<'s, v8::Array>> {
    let pk = v8::String::new(scope, "__pairs").unwrap();
    let hidden_key = v8::Private::for_api(scope, Some(pk));
    let val = this.get_private(scope, hidden_key)?;
    v8::Local::<v8::Array>::try_from(val).ok()
}
fn fd_append(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue) {
    let Some(pairs) = fd_get_pairs(scope, args.this()) else { return };
    let pair = v8::Array::new(scope, 2);
    pair.set_index(scope, 0, args.get(0));
    pair.set_index(scope, 1, args.get(1));
    pairs.set_index(scope, pairs.length(), pair.into());
}
fn fd_get(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let key = args.get(0).to_rust_string_lossy(scope);
    let Some(pairs) = fd_get_pairs(scope, args.this()) else { return };
    for i in 0..pairs.length() {
        if let Some(p) = pairs.get_index(scope, i) {
            let p = unsafe { v8::Local::<v8::Array>::cast_unchecked(p) };
            if let Some(k) = p.get_index(scope, 0) {
                if k.to_rust_string_lossy(scope) == key {
                    if let Some(v) = p.get_index(scope, 1) { rv.set(v); return; }
                }
            }
        }
    }
    rv.set(v8::null(scope).into());
}
fn fd_get_all(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let key = args.get(0).to_rust_string_lossy(scope);
    let Some(pairs) = fd_get_pairs(scope, args.this()) else { return };
    let result = v8::Array::new(scope, 0);
    let mut idx = 0u32;
    for i in 0..pairs.length() {
        if let Some(p) = pairs.get_index(scope, i) {
            let p = unsafe { v8::Local::<v8::Array>::cast_unchecked(p) };
            if let Some(k) = p.get_index(scope, 0) {
                if k.to_rust_string_lossy(scope) == key {
                    if let Some(v) = p.get_index(scope, 1) { result.set_index(scope, idx, v); idx += 1; }
                }
            }
        }
    }
    rv.set(result.into());
}
fn fd_set(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue) {
    // Per XHR spec: set() replaces the first entry with the given name and removes all others.
    let key = args.get(0).to_rust_string_lossy(scope);
    let val = args.get(1);
    let Some(pairs) = fd_get_pairs(scope, args.this()) else { return };
    let new_pairs = v8::Array::new(scope, 0);
    let mut idx = 0u32;
    let mut found = false;
    for i in 0..pairs.length() {
        if let Some(p) = pairs.get_index(scope, i) {
            let p_arr = unsafe { v8::Local::<v8::Array>::cast_unchecked(p) };
            if let Some(k) = p_arr.get_index(scope, 0) {
                if k.to_rust_string_lossy(scope) == key {
                    if !found {
                        p_arr.set_index(scope, 1, val);
                        new_pairs.set_index(scope, idx, p);
                        idx += 1;
                        found = true;
                    }
                    // else: remove duplicate entries per spec
                } else {
                    new_pairs.set_index(scope, idx, p);
                    idx += 1;
                }
            }
        }
    }
    if !found {
        let pair = v8::Array::new(scope, 2);
        let ks = v8::String::new(scope, &key).unwrap();
        pair.set_index(scope, 0, ks.into());
        pair.set_index(scope, 1, val);
        new_pairs.set_index(scope, idx, pair.into());
    }
    let pk = v8::String::new(scope, "__pairs").unwrap();
    let hidden_key = v8::Private::for_api(scope, Some(pk));
    args.this().set_private(scope, hidden_key, new_pairs.into());
}
fn fd_has(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let key = args.get(0).to_rust_string_lossy(scope);
    let Some(pairs) = fd_get_pairs(scope, args.this()) else { return };
    for i in 0..pairs.length() {
        if let Some(p) = pairs.get_index(scope, i) {
            let p = unsafe { v8::Local::<v8::Array>::cast_unchecked(p) };
            if let Some(k) = p.get_index(scope, 0) {
                if k.to_rust_string_lossy(scope) == key { rv.set(v8::Boolean::new(scope, true).into()); return; }
            }
        }
    }
    rv.set(v8::Boolean::new(scope, false).into());
}
fn fd_delete(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue) {
    let key = args.get(0).to_rust_string_lossy(scope);
    let Some(pairs) = fd_get_pairs(scope, args.this()) else { return };
    let new_pairs = v8::Array::new(scope, 0);
    let mut idx = 0u32;
    for i in 0..pairs.length() {
        if let Some(p) = pairs.get_index(scope, i) {
            let pa = unsafe { v8::Local::<v8::Array>::cast_unchecked(p) };
            if let Some(k) = pa.get_index(scope, 0) {
                if k.to_rust_string_lossy(scope) != key { new_pairs.set_index(scope, idx, p); idx += 1; }
            }
        }
    }
    let pk = v8::String::new(scope, "__pairs").unwrap();
    let hidden_key = v8::Private::for_api(scope, Some(pk));
    args.this().set_private(scope, hidden_key, new_pairs.into());
}
pub fn fd_entries(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let Some(pairs) = fd_get_pairs(scope, args.this()) else { return };
    let arr = v8::Array::new(scope, pairs.length() as i32);
    for i in 0..pairs.length() { if let Some(p) = pairs.get_index(scope, i) { arr.set_index(scope, i, p); } }
    rv.set(arr.into());
}
pub fn fd_keys(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let Some(pairs) = fd_get_pairs(scope, args.this()) else { return };
    let arr = v8::Array::new(scope, pairs.length() as i32);
    for i in 0..pairs.length() {
        if let Some(p) = pairs.get_index(scope, i) {
            let p = unsafe { v8::Local::<v8::Array>::cast_unchecked(p) };
            if let Some(k) = p.get_index(scope, 0) { arr.set_index(scope, i, k); }
        }
    }
    rv.set(arr.into());
}
pub fn fd_values(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let Some(pairs) = fd_get_pairs(scope, args.this()) else { return };
    let arr = v8::Array::new(scope, pairs.length() as i32);
    for i in 0..pairs.length() {
        if let Some(p) = pairs.get_index(scope, i) {
            let p = unsafe { v8::Local::<v8::Array>::cast_unchecked(p) };
            if let Some(v) = p.get_index(scope, 1) { arr.set_index(scope, i, v); }
        }
    }
    rv.set(arr.into());
}
pub fn fd_for_each(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue) {
    let cb = args.get(0);
    if !cb.is_function() { return; }
    let func = unsafe { v8::Local::<v8::Function>::cast_unchecked(cb) };
    let Some(pairs) = fd_get_pairs(scope, args.this()) else { return };
    let undef = v8::undefined(scope);
    for i in 0..pairs.length() {
        if let Some(p) = pairs.get_index(scope, i) {
            let p = unsafe { v8::Local::<v8::Array>::cast_unchecked(p) };
            let k = p.get_index(scope, 0).unwrap_or_else(|| v8::undefined(scope).into());
            let v = p.get_index(scope, 1).unwrap_or_else(|| v8::undefined(scope).into());
            func.call(scope, undef.into(), &[v, k, args.this().into()]);
        }
    }
}
