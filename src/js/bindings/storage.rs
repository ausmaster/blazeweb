/// localStorage / sessionStorage — in-memory implementations for SSR.

use std::collections::HashMap;

/// In-memory storage for both localStorage and sessionStorage.
pub struct WebStorage {
    pub local: HashMap<String, String>,
    pub session: HashMap<String, String>,
}

impl WebStorage {
    pub fn new() -> Self {
        Self {
            local: HashMap::new(),
            session: HashMap::new(),
        }
    }
}

/// Create a Storage-like object backed by the given store type.
pub fn create_storage_object<'s>(
    scope: &mut v8::HandleScope<'s>,
    is_local: bool,
) -> v8::Local<'s, v8::Object> {
    let obj = v8::Object::new(scope);

    // Store which type in a private key
    let name = v8::String::new(scope, "__storageType").unwrap();
    let type_key = v8::Private::for_api(scope, Some(name));
    let type_val = v8::Boolean::new(scope, is_local);
    obj.set_private(scope, type_key, type_val.into());

    // getItem
    let get_item = v8::Function::new(scope, storage_get_item).unwrap();
    let k = v8::String::new(scope, "getItem").unwrap();
    obj.set(scope, k.into(), get_item.into());

    // setItem
    let set_item = v8::Function::new(scope, storage_set_item).unwrap();
    let k = v8::String::new(scope, "setItem").unwrap();
    obj.set(scope, k.into(), set_item.into());

    // removeItem
    let remove_item = v8::Function::new(scope, storage_remove_item).unwrap();
    let k = v8::String::new(scope, "removeItem").unwrap();
    obj.set(scope, k.into(), remove_item.into());

    // clear
    let clear = v8::Function::new(scope, storage_clear).unwrap();
    let k = v8::String::new(scope, "clear").unwrap();
    obj.set(scope, k.into(), clear.into());

    // key
    let key_fn = v8::Function::new(scope, storage_key).unwrap();
    let k = v8::String::new(scope, "key").unwrap();
    obj.set(scope, k.into(), key_fn.into());

    // length — per Web Storage spec, this is a readonly getter property (not a method)
    let k = v8::String::new(scope, "length").unwrap();
    obj.set_accessor(scope, k.into(), storage_length_getter);

    obj
}

fn is_local_storage(scope: &mut v8::HandleScope, this: v8::Local<v8::Object>) -> bool {
    let name = v8::String::new(scope, "__storageType").unwrap();
    let type_key = v8::Private::for_api(scope, Some(name));
    this.get_private(scope, type_key)
        .map(|v| v.boolean_value(scope))
        .unwrap_or(true)
}

fn storage_get_item(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let key = args.get(0).to_rust_string_lossy(scope);
    let is_local = is_local_storage(scope, args.this());

    let val = {
        let ws = scope.get_slot::<WebStorage>().unwrap();
        let store = if is_local { &ws.local } else { &ws.session };
        store.get(&key).cloned()
    };
    match val {
        Some(val) => {
            let v = v8::String::new(scope, &val).unwrap();
            rv.set(v.into());
        }
        None => rv.set(v8::null(scope).into()),
    }
}

fn storage_set_item(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let key = args.get(0).to_rust_string_lossy(scope);
    let value = args.get(1).to_rust_string_lossy(scope);
    let is_local = is_local_storage(scope, args.this());

    let ws = scope.get_slot_mut::<WebStorage>().unwrap();
    let store = if is_local { &mut ws.local } else { &mut ws.session };
    store.insert(key, value);
}

fn storage_remove_item(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let key = args.get(0).to_rust_string_lossy(scope);
    let is_local = is_local_storage(scope, args.this());

    let ws = scope.get_slot_mut::<WebStorage>().unwrap();
    let store = if is_local { &mut ws.local } else { &mut ws.session };
    store.remove(&key);
}

fn storage_clear(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let is_local = {
        let _args_this = _args.this();
        is_local_storage(scope, _args_this)
    };

    let ws = scope.get_slot_mut::<WebStorage>().unwrap();
    let store = if is_local { &mut ws.local } else { &mut ws.session };
    store.clear();
}

fn storage_key(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let index = args.get(0).int32_value(scope).unwrap_or(0) as usize;
    let is_local = is_local_storage(scope, args.this());

    let key_val = {
        let ws = scope.get_slot::<WebStorage>().unwrap();
        let store = if is_local { &ws.local } else { &ws.session };
        store.keys().nth(index).cloned()
    };
    match key_val {
        Some(key) => {
            let v = v8::String::new(scope, &key).unwrap();
            rv.set(v.into());
        }
        None => rv.set(v8::null(scope).into()),
    }
}

fn storage_length_getter(
    scope: &mut v8::HandleScope,
    _key: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let is_local = is_local_storage(scope, args.this());

    let len = {
        let ws = scope.get_slot::<WebStorage>().unwrap();
        let store = if is_local { &ws.local } else { &ws.session };
        store.len() as i32
    };
    rv.set(v8::Integer::new(scope, len).into());
}
