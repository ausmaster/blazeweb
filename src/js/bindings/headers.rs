/// Headers API — case-insensitive key/value store for HTTP headers.

use super::formdata::{fd_get_pairs, fd_for_each, fd_entries, fd_keys, fd_values};

/// Install the Headers constructor on the global object.
pub fn install(scope: &mut v8::PinnedRef<v8::HandleScope>, global: v8::Local<v8::Object>) {
    let headers_ctor = v8::Function::new(scope, headers_constructor).unwrap();
    let key = v8::String::new(scope, "Headers").unwrap();
    global.set(scope, key.into(), headers_ctor.into());
}

fn headers_constructor(scope: &mut v8::PinnedRef<v8::HandleScope>, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let obj = v8::Object::new(scope);
    let pairs = v8::Array::new(scope, 0);
    let pk = v8::String::new(scope, "__pairs").unwrap();
    let hidden_key = v8::Private::for_api(scope, Some(pk));
    obj.set_private(scope, hidden_key, pairs.into());

    if args.length() > 0 && args.get(0).is_object() && !args.get(0).is_array() {
        let init = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(0)) };
        if let Some(names) = init.get_own_property_names(scope, v8::GetPropertyNamesArgs::default()) {
            for i in 0..names.length() {
                if let Some(key) = names.get_index(scope, i) {
                    let k_str = key.to_rust_string_lossy(scope).to_ascii_lowercase();
                    if let Some(val) = init.get(scope, key) {
                        let pair = v8::Array::new(scope, 2);
                        let ks = v8::String::new(scope, &k_str).unwrap();
                        pair.set_index(scope, 0, ks.into());
                        pair.set_index(scope, 1, val);
                        pairs.set_index(scope, pairs.length(), pair.into());
                    }
                }
            }
        }
    }

    let get_fn = v8::Function::new(scope, headers_get).unwrap();
    let k = v8::String::new(scope, "get").unwrap();
    obj.set(scope, k.into(), get_fn.into());
    let set_fn = v8::Function::new(scope, headers_set).unwrap();
    let k = v8::String::new(scope, "set").unwrap();
    obj.set(scope, k.into(), set_fn.into());
    let has_fn = v8::Function::new(scope, headers_has).unwrap();
    let k = v8::String::new(scope, "has").unwrap();
    obj.set(scope, k.into(), has_fn.into());
    let append_fn = v8::Function::new(scope, headers_append).unwrap();
    let k = v8::String::new(scope, "append").unwrap();
    obj.set(scope, k.into(), append_fn.into());
    let delete_fn = v8::Function::new(scope, headers_delete).unwrap();
    let k = v8::String::new(scope, "delete").unwrap();
    obj.set(scope, k.into(), delete_fn.into());
    let foreach_fn = v8::Function::new(scope, fd_for_each).unwrap();
    let k = v8::String::new(scope, "forEach").unwrap();
    obj.set(scope, k.into(), foreach_fn.into());
    let entries_fn = v8::Function::new(scope, fd_entries).unwrap();
    let k = v8::String::new(scope, "entries").unwrap();
    obj.set(scope, k.into(), entries_fn.into());
    let keys_fn = v8::Function::new(scope, fd_keys).unwrap();
    let k = v8::String::new(scope, "keys").unwrap();
    obj.set(scope, k.into(), keys_fn.into());
    let values_fn = v8::Function::new(scope, fd_values).unwrap();
    let k = v8::String::new(scope, "values").unwrap();
    obj.set(scope, k.into(), values_fn.into());
    rv.set(obj.into());
}
fn headers_get(scope: &mut v8::PinnedRef<v8::HandleScope>, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let key = args.get(0).to_rust_string_lossy(scope).to_ascii_lowercase();
    let Some(pairs) = fd_get_pairs(scope, args.this()) else { return };
    for i in 0..pairs.length() {
        if let Some(p) = pairs.get_index(scope, i) {
            let p = unsafe { v8::Local::<v8::Array>::cast_unchecked(p) };
            if let Some(k) = p.get_index(scope, 0) {
                if k.to_rust_string_lossy(scope).to_ascii_lowercase() == key {
                    if let Some(v) = p.get_index(scope, 1) { rv.set(v); return; }
                }
            }
        }
    }
    rv.set(v8::null(scope).into());
}
fn headers_has(scope: &mut v8::PinnedRef<v8::HandleScope>, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let key = args.get(0).to_rust_string_lossy(scope).to_ascii_lowercase();
    let Some(pairs) = fd_get_pairs(scope, args.this()) else { return };
    for i in 0..pairs.length() {
        if let Some(p) = pairs.get_index(scope, i) {
            let p = unsafe { v8::Local::<v8::Array>::cast_unchecked(p) };
            if let Some(k) = p.get_index(scope, 0) {
                if k.to_rust_string_lossy(scope).to_ascii_lowercase() == key { rv.set(v8::Boolean::new(scope, true).into()); return; }
            }
        }
    }
    rv.set(v8::Boolean::new(scope, false).into());
}
fn headers_set(scope: &mut v8::PinnedRef<v8::HandleScope>, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue) {
    let key = args.get(0).to_rust_string_lossy(scope).to_ascii_lowercase();
    let val = args.get(1);
    let Some(pairs) = fd_get_pairs(scope, args.this()) else { return };
    let mut found = false;
    for i in 0..pairs.length() {
        if let Some(p) = pairs.get_index(scope, i) {
            let p = unsafe { v8::Local::<v8::Array>::cast_unchecked(p) };
            if let Some(k) = p.get_index(scope, 0) {
                if k.to_rust_string_lossy(scope).to_ascii_lowercase() == key && !found {
                    let ks = v8::String::new(scope, &key).unwrap();
                    p.set_index(scope, 0, ks.into());
                    p.set_index(scope, 1, val);
                    found = true;
                }
            }
        }
    }
    if !found {
        let pair = v8::Array::new(scope, 2);
        let ks = v8::String::new(scope, &key).unwrap();
        pair.set_index(scope, 0, ks.into());
        pair.set_index(scope, 1, val);
        pairs.set_index(scope, pairs.length(), pair.into());
    }
}
fn headers_append(scope: &mut v8::PinnedRef<v8::HandleScope>, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue) {
    let key = args.get(0).to_rust_string_lossy(scope).to_ascii_lowercase();
    let val = args.get(1);
    let Some(pairs) = fd_get_pairs(scope, args.this()) else { return };
    let pair = v8::Array::new(scope, 2);
    let ks = v8::String::new(scope, &key).unwrap();
    pair.set_index(scope, 0, ks.into());
    pair.set_index(scope, 1, val);
    pairs.set_index(scope, pairs.length(), pair.into());
}
fn headers_delete(scope: &mut v8::PinnedRef<v8::HandleScope>, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue) {
    let key = args.get(0).to_rust_string_lossy(scope).to_ascii_lowercase();
    let Some(pairs) = fd_get_pairs(scope, args.this()) else { return };
    let new_pairs = v8::Array::new(scope, 0);
    let mut idx = 0u32;
    for i in 0..pairs.length() {
        if let Some(p) = pairs.get_index(scope, i) {
            let pa = unsafe { v8::Local::<v8::Array>::cast_unchecked(p) };
            if let Some(k) = pa.get_index(scope, 0) {
                if k.to_rust_string_lossy(scope).to_ascii_lowercase() != key { new_pairs.set_index(scope, idx, p); idx += 1; }
            }
        }
    }
    let pk = v8::String::new(scope, "__pairs").unwrap();
    let hidden_key = v8::Private::for_api(scope, Some(pk));
    args.this().set_private(scope, hidden_key, new_pairs.into());
}
