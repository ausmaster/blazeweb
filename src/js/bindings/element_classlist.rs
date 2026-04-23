/// Element classList bindings.

use crate::dom::node::NodeData;
use crate::js::templates::{arena_mut, arena_ref, unwrap_node_id};

pub(super) fn class_list_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };

    let obj = v8::Object::new(scope);

    // Store node_id in private key
    let boxed = Box::new(node_id);
    let external = v8::External::new(scope, Box::into_raw(boxed) as *mut std::ffi::c_void);
    let pk_name = v8::String::new(scope, "__nodeId").unwrap();
    let hidden_key = v8::Private::for_api(scope, Some(pk_name));
    obj.set_private(scope, hidden_key, external.into());

    // add(...tokens)
    let add_fn = v8::Function::new(scope, classlist_add).unwrap();
    let k = v8::String::new(scope, "add").unwrap();
    obj.set(scope, k.into(), add_fn.into());

    // remove(...tokens)
    let remove_fn = v8::Function::new(scope, classlist_remove).unwrap();
    let k = v8::String::new(scope, "remove").unwrap();
    obj.set(scope, k.into(), remove_fn.into());

    // toggle(token, force?)
    let toggle_fn = v8::Function::new(scope, classlist_toggle).unwrap();
    let k = v8::String::new(scope, "toggle").unwrap();
    obj.set(scope, k.into(), toggle_fn.into());

    // contains(token)
    let contains_fn = v8::Function::new(scope, classlist_contains).unwrap();
    let k = v8::String::new(scope, "contains").unwrap();
    obj.set(scope, k.into(), contains_fn.into());

    // item(index)
    let item_fn = v8::Function::new(scope, classlist_item).unwrap();
    let k = v8::String::new(scope, "item").unwrap();
    obj.set(scope, k.into(), item_fn.into());

    // replace(old, new)
    let replace_fn = v8::Function::new(scope, classlist_replace).unwrap();
    let k = v8::String::new(scope, "replace").unwrap();
    obj.set(scope, k.into(), replace_fn.into());

    // toString
    let to_string_fn = v8::Function::new(scope, classlist_to_string).unwrap();
    let k = v8::String::new(scope, "toString").unwrap();
    obj.set(scope, k.into(), to_string_fn.into());

    // length
    let arena = arena_ref(scope);
    let count = if let NodeData::Element(data) = &arena.nodes[node_id].data {
        data.get_attribute("class")
            .map(|s| s.split_whitespace().count() as i32)
            .unwrap_or(0)
    } else {
        0
    };
    let k = v8::String::new(scope, "length").unwrap();
    let v = v8::Integer::new(scope, count);
    obj.set(scope, k.into(), v.into());

    // value (same as className)
    let arena2 = arena_ref(scope);
    let class_val = if let NodeData::Element(data) = &arena2.nodes[node_id].data {
        data.get_attribute("class").unwrap_or("").to_string()
    } else {
        String::new()
    };
    let k = v8::String::new(scope, "value").unwrap();
    let v = v8::String::new(scope, &class_val).unwrap();
    obj.set(scope, k.into(), v.into());

    rv.set(obj.into());
}

pub(super) fn get_classlist_node_id(scope: &mut v8::PinnedRef<v8::HandleScope>, this: v8::Local<v8::Object>) -> Option<crate::dom::NodeId> {
    let pk_name = v8::String::new(scope, "__nodeId").unwrap();
    let hidden_key = v8::Private::for_api(scope, Some(pk_name));
    let val = this.get_private(scope, hidden_key)?;
    let ext = v8::Local::<v8::External>::try_from(val).ok()?;
    let ptr = ext.value() as *const crate::dom::NodeId;
    if ptr.is_null() { return None; }
    Some(unsafe { *ptr })
}

pub(super) fn classlist_add(scope: &mut v8::PinnedRef<v8::HandleScope>, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue) {
    let Some(node_id) = get_classlist_node_id(scope, args.this()) else { return };
    let mut tokens = Vec::new();
    for i in 0..args.length() {
        tokens.push(args.get(i).to_rust_string_lossy(scope));
    }
    let arena = arena_mut(scope);
    if let NodeData::Element(data) = &mut arena.nodes[node_id].data {
        let current = data.get_attribute("class").unwrap_or("").to_string();
        let mut classes: Vec<String> = current.split_whitespace().map(|s| s.to_string()).collect();
        for tok in &tokens {
            if !classes.iter().any(|c| c == tok) {
                classes.push(tok.clone());
            }
        }
        data.set_attribute("class", &classes.join(" "));
    }
}

pub(super) fn classlist_remove(scope: &mut v8::PinnedRef<v8::HandleScope>, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue) {
    let Some(node_id) = get_classlist_node_id(scope, args.this()) else { return };
    let mut tokens = Vec::new();
    for i in 0..args.length() {
        tokens.push(args.get(i).to_rust_string_lossy(scope));
    }
    let arena = arena_mut(scope);
    if let NodeData::Element(data) = &mut arena.nodes[node_id].data {
        let current = data.get_attribute("class").unwrap_or("").to_string();
        let classes: Vec<&str> = current.split_whitespace()
            .filter(|c| !tokens.iter().any(|t| t == c))
            .collect();
        if classes.is_empty() {
            data.remove_attribute("class");
        } else {
            data.set_attribute("class", &classes.join(" "));
        }
    }
}

pub(super) fn classlist_toggle(scope: &mut v8::PinnedRef<v8::HandleScope>, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let Some(node_id) = get_classlist_node_id(scope, args.this()) else { return };
    let token = args.get(0).to_rust_string_lossy(scope);
    let force = if args.length() > 1 && !args.get(1).is_undefined() {
        Some(args.get(1).boolean_value(scope))
    } else {
        None
    };
    let arena = arena_mut(scope);
    if let NodeData::Element(data) = &mut arena.nodes[node_id].data {
        let current = data.get_attribute("class").unwrap_or("").to_string();
        let mut classes: Vec<String> = current.split_whitespace().map(|s| s.to_string()).collect();
        let has = classes.iter().any(|c| *c == token);
        let result = match force {
            Some(true) => {
                if !has { classes.push(token); }
                true
            }
            Some(false) => {
                classes.retain(|c| *c != token);
                false
            }
            None => {
                if has {
                    classes.retain(|c| *c != token);
                    false
                } else {
                    classes.push(token);
                    true
                }
            }
        };
        if classes.is_empty() {
            data.remove_attribute("class");
        } else {
            data.set_attribute("class", &classes.join(" "));
        }
        rv.set(v8::Boolean::new(scope, result).into());
    }
}

pub(super) fn classlist_contains(scope: &mut v8::PinnedRef<v8::HandleScope>, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let Some(node_id) = get_classlist_node_id(scope, args.this()) else { return };
    let token = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_ref(scope);
    let has = if let NodeData::Element(data) = &arena.nodes[node_id].data {
        data.get_attribute("class").unwrap_or("").split_whitespace().any(|c| c == token)
    } else {
        false
    };
    rv.set(v8::Boolean::new(scope, has).into());
}

pub(super) fn classlist_item(scope: &mut v8::PinnedRef<v8::HandleScope>, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let Some(node_id) = get_classlist_node_id(scope, args.this()) else { return };
    let index = args.get(0).int32_value(scope).unwrap_or(0) as usize;
    let arena = arena_ref(scope);
    if let NodeData::Element(data) = &arena.nodes[node_id].data {
        if let Some(cls) = data.get_attribute("class").unwrap_or("").split_whitespace().nth(index) {
            let v = v8::String::new(scope, cls).unwrap();
            rv.set(v.into());
            return;
        }
    }
    rv.set(v8::null(scope).into());
}

pub(super) fn classlist_replace(scope: &mut v8::PinnedRef<v8::HandleScope>, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let Some(node_id) = get_classlist_node_id(scope, args.this()) else { return };
    let old_token = args.get(0).to_rust_string_lossy(scope);
    let new_token = args.get(1).to_rust_string_lossy(scope);
    let arena = arena_mut(scope);
    if let NodeData::Element(data) = &mut arena.nodes[node_id].data {
        let current = data.get_attribute("class").unwrap_or("").to_string();
        let mut classes: Vec<String> = current.split_whitespace().map(|s| s.to_string()).collect();
        let mut replaced = false;
        for cls in &mut classes {
            if *cls == old_token {
                *cls = new_token.clone();
                replaced = true;
                break;
            }
        }
        data.set_attribute("class", &classes.join(" "));
        rv.set(v8::Boolean::new(scope, replaced).into());
    }
}

pub(super) fn classlist_to_string(scope: &mut v8::PinnedRef<v8::HandleScope>, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let Some(node_id) = get_classlist_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let cls = if let NodeData::Element(data) = &arena.nodes[node_id].data {
        data.get_attribute("class").unwrap_or("").to_string()
    } else {
        String::new()
    };
    let v = v8::String::new(scope, &cls).unwrap();
    rv.set(v.into());
}
