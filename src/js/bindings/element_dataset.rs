/// Element dataset bindings — DOMStringMap proxy.

use crate::dom::node::NodeData;
use crate::js::templates::{arena_mut, arena_ref, unwrap_node_id};

pub(super) fn dataset_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let target = v8::Object::new(scope);

    // Store node_id for the proxy set handler
    let boxed = Box::new(node_id);
    let external = v8::External::new(scope, Box::into_raw(boxed) as *mut std::ffi::c_void);
    let pk_name = v8::String::new(scope, "__nodeId").unwrap();
    let hidden_key = v8::Private::for_api(scope, Some(pk_name));
    target.set_private(scope, hidden_key, external.into());

    let arena = arena_ref(scope);
    if let NodeData::Element(data) = &arena.nodes[node_id].data {
        for attr in &data.attrs {
            let name = &*attr.name.local;
            if let Some(data_name) = name.strip_prefix("data-") {
                let camel = data_attr_to_camel(data_name);
                let k = v8::String::new(scope, &camel).unwrap();
                let v = v8::String::new(scope, &attr.value).unwrap();
                target.set(scope, k.into(), v.into());
            }
        }
    }

    // Wrap in Proxy for write-through to data-* attributes
    let set_fn = v8::Function::new(scope, dataset_proxy_set).unwrap();
    let handler = v8::Object::new(scope);
    let set_key = v8::String::new(scope, "set").unwrap();
    handler.set(scope, set_key.into(), set_fn.into());

    if let Some(proxy) = v8::Proxy::new(scope, target.into(), handler.into()) {
        rv.set(proxy.into());
    } else {
        rv.set(target.into());
    }
}

fn dataset_proxy_set(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let target = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(0)) };
    let prop = args.get(1);
    let value = args.get(2);

    // Set on target
    target.set(scope, prop, value);
    rv.set(v8::Boolean::new(scope, true).into());

    if !prop.is_string() { return; }

    let camel = prop.to_rust_string_lossy(scope);
    let kebab = camel_to_data_attr(&camel);
    let value_str = value.to_rust_string_lossy(scope);

    let pk_name = v8::String::new(scope, "__nodeId").unwrap();
    let hidden_key = v8::Private::for_api(scope, Some(pk_name));
    let Some(ext_val) = target.get_private(scope, hidden_key) else { return };
    let Ok(ext) = v8::Local::<v8::External>::try_from(ext_val) else { return };
    let ptr = ext.value() as *const crate::dom::NodeId;
    if ptr.is_null() { return; }
    let node_id = unsafe { *ptr };

    let arena = arena_mut(scope);
    if let NodeData::Element(data) = &mut arena.nodes[node_id].data {
        data.set_attribute(&kebab, &value_str);
    }
}

/// Convert camelCase dataset key to data-kebab-case attribute name.
fn camel_to_data_attr(s: &str) -> String {
    let mut result = String::from("data-");
    for c in s.chars() {
        if c.is_uppercase() {
            result.push('-');
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}

fn data_attr_to_camel(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut next_upper = false;
    for c in s.chars() {
        if c == '-' {
            next_upper = true;
        } else if next_upper {
            result.push(c.to_uppercase().next().unwrap());
            next_upper = false;
        } else {
            result.push(c);
        }
    }
    result
}
