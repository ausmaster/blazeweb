/// CSSStyleDeclaration-like `style` property on elements.
///
/// Reading/writing style properties goes through the element's "style" attribute.
/// Supports both camelCase (`backgroundColor`) and kebab-case (`background-color`).

use crate::dom::node::NodeData;
use crate::js::templates::{arena_mut, arena_ref, unwrap_node_id};

/// Install the `style` accessor on the Element prototype.
pub fn install(scope: &mut v8::HandleScope<()>, proto: &v8::Local<v8::ObjectTemplate>) {
    let key = v8::String::new(scope, "style").unwrap();
    let getter_ft = v8::FunctionTemplate::new(scope, style_getter);
    proto.set_accessor_property(key.into(), Some(getter_ft), None, v8::PropertyAttribute::NONE);
}

fn style_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };

    // Create a target object with the node_id and methods
    let target = v8::Object::new(scope);

    // Store the node_id so methods can find the element
    let boxed = Box::new(node_id);
    let external = v8::External::new(scope, Box::into_raw(boxed) as *mut std::ffi::c_void);
    let name = v8::String::new(scope, "__nodeId").unwrap();
    let hidden_key = v8::Private::for_api(scope, Some(name));
    target.set_private(scope, hidden_key, external.into());

    // Populate current style properties from the style attribute
    let arena = arena_ref(scope);
    if let NodeData::Element(data) = &arena.nodes[node_id].data {
        if let Some(style_str) = data.get_attribute("style") {
            for decl in parse_style_attribute(style_str) {
                let camel = kebab_to_camel(&decl.0);
                let k = v8::String::new(scope, &camel).unwrap();
                let v = v8::String::new(scope, &decl.1).unwrap();
                target.set(scope, k.into(), v.into());
                // Also set kebab-case
                let k2 = v8::String::new(scope, &decl.0).unwrap();
                target.set(scope, k2.into(), v.into());
            }
        }
    }

    // getPropertyValue
    let gpv = v8::Function::new(scope, get_property_value).unwrap();
    let k = v8::String::new(scope, "getPropertyValue").unwrap();
    target.set(scope, k.into(), gpv.into());

    // setProperty
    let sp = v8::Function::new(scope, set_property).unwrap();
    let k = v8::String::new(scope, "setProperty").unwrap();
    target.set(scope, k.into(), sp.into());

    // removeProperty
    let rp = v8::Function::new(scope, remove_property).unwrap();
    let k = v8::String::new(scope, "removeProperty").unwrap();
    target.set(scope, k.into(), rp.into());

    // cssText
    let arena2 = arena_ref(scope);
    if let NodeData::Element(data) = &arena2.nodes[node_id].data {
        let css_text = data.get_attribute("style").unwrap_or("");
        let k = v8::String::new(scope, "cssText").unwrap();
        let v = v8::String::new(scope, css_text).unwrap();
        target.set(scope, k.into(), v.into());
    }

    // length
    let arena3 = arena_ref(scope);
    let count = if let NodeData::Element(data) = &arena3.nodes[node_id].data {
        data.get_attribute("style")
            .map(|s| parse_style_attribute(s).len() as i32)
            .unwrap_or(0)
    } else {
        0
    };
    let k = v8::String::new(scope, "length").unwrap();
    let v = v8::Integer::new(scope, count);
    target.set(scope, k.into(), v.into());

    // Wrap in a Proxy to intercept property sets and write-through to the style attribute
    let proxy_set_fn = v8::Function::new(scope, style_proxy_set).unwrap();
    let proxy_get_fn = v8::Function::new(scope, style_proxy_get).unwrap();
    let handler = v8::Object::new(scope);
    let set_key = v8::String::new(scope, "set").unwrap();
    handler.set(scope, set_key.into(), proxy_set_fn.into());
    let get_key = v8::String::new(scope, "get").unwrap();
    handler.set(scope, get_key.into(), proxy_get_fn.into());

    // Create proxy
    if let Some(proxy) = v8::Proxy::new(scope, target.into(), handler.into()) {
        rv.set(proxy.into());
    } else {
        rv.set(target.into());
    }
}

/// Proxy get handler — reads from the target for functions, or reads
/// live from the DOM style attribute for property values.
fn style_proxy_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let target = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(0)) };
    let prop = args.get(1);

    // Let functions and symbols pass through from target
    if !prop.is_string() {
        if let Some(val) = target.get(scope, prop) {
            rv.set(val);
        }
        return;
    }

    let prop_str = prop.to_rust_string_lossy(scope);

    // Methods and special properties: read from target
    match prop_str.as_str() {
        "getPropertyValue" | "setProperty" | "removeProperty" | "__nodeId" => {
            if let Some(val) = target.get(scope, prop) {
                rv.set(val);
            }
            return;
        }
        _ => {}
    }

    // For cssText and length, read live from the DOM
    let Some(node_id) = get_node_id_from_style(scope, target) else {
        if let Some(val) = target.get(scope, prop) {
            rv.set(val);
        }
        return;
    };

    match prop_str.as_str() {
        "cssText" => {
            let arena = arena_ref(scope);
            let val = if let NodeData::Element(data) = &arena.nodes[node_id].data {
                data.get_attribute("style").unwrap_or("").to_string()
            } else {
                String::new()
            };
            let v = v8::String::new(scope, &val).unwrap();
            rv.set(v.into());
        }
        "length" => {
            let arena = arena_ref(scope);
            let count = if let NodeData::Element(data) = &arena.nodes[node_id].data {
                data.get_attribute("style")
                    .map(|s| parse_style_attribute(s).len() as i32)
                    .unwrap_or(0)
            } else {
                0
            };
            rv.set(v8::Integer::new(scope, count).into());
        }
        _ => {
            // Read live from the DOM style attribute
            let kebab = camel_to_kebab(&prop_str);
            let arena = arena_ref(scope);
            let val = if let NodeData::Element(data) = &arena.nodes[node_id].data {
                if let Some(style_str) = data.get_attribute("style") {
                    parse_style_attribute(style_str)
                        .iter()
                        .find(|(k, _)| *k == kebab)
                        .map(|(_, v)| v.clone())
                        .unwrap_or_default()
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            let v = v8::String::new(scope, &val).unwrap();
            rv.set(v.into());
        }
    }
}

/// Proxy set handler — intercepts property assignments and writes
/// them through to the element's style attribute in the DOM.
fn style_proxy_set(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let target = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(0)) };
    let prop = args.get(1);
    let value = args.get(2);

    // Always set on target
    target.set(scope, prop, value);
    rv.set(v8::Boolean::new(scope, true).into());

    // If prop is not a string, don't write to style attr
    if !prop.is_string() {
        return;
    }

    let prop_str = prop.to_rust_string_lossy(scope);

    // Skip methods and internal props
    match prop_str.as_str() {
        "getPropertyValue" | "setProperty" | "removeProperty" | "length" => return,
        _ => {}
    }

    let Some(node_id) = get_node_id_from_style(scope, target) else { return };
    let value_str = value.to_rust_string_lossy(scope);

    // Handle cssText separately — replace entire style attribute
    if prop_str == "cssText" {
        let arena = arena_mut(scope);
        if let NodeData::Element(data) = &mut arena.nodes[node_id].data {
            if value_str.is_empty() {
                data.remove_attribute("style");
            } else {
                data.set_attribute("style", &value_str);
            }
        }
        return;
    }

    // Convert camelCase to kebab-case and update the style attribute
    let kebab = camel_to_kebab(&prop_str);
    let arena = arena_mut(scope);
    if let NodeData::Element(data) = &mut arena.nodes[node_id].data {
        let mut props = if let Some(style_str) = data.get_attribute("style") {
            parse_style_attribute(style_str)
        } else {
            vec![]
        };

        if value_str.is_empty() {
            // Remove the property
            props.retain(|(k, _)| *k != kebab);
        } else {
            // Update or add
            let mut found = false;
            for (k, v) in &mut props {
                if *k == kebab {
                    *v = value_str.clone();
                    found = true;
                    break;
                }
            }
            if !found {
                props.push((kebab, value_str));
            }
        }

        if props.is_empty() {
            data.remove_attribute("style");
        } else {
            let new_style = serialize_style_props(&props);
            data.set_attribute("style", &new_style);
        }
    }
}

fn get_node_id_from_style(scope: &mut v8::HandleScope, this: v8::Local<v8::Object>) -> Option<crate::dom::NodeId> {
    let name = v8::String::new(scope, "__nodeId").unwrap();
    let hidden_key = v8::Private::for_api(scope, Some(name));

    // Try on this first, then unwrap Proxy to get target
    let obj = if let Some(val) = this.get_private(scope, hidden_key) {
        if !val.is_undefined() {
            return extract_node_id_from_external(val);
        }
        // Might be a Proxy — try to get target
        if this.is_proxy() {
            let proxy = unsafe { v8::Local::<v8::Proxy>::cast_unchecked(this) };
            let target_val = proxy.get_target(scope);
            if target_val.is_object() {
                unsafe { v8::Local::<v8::Object>::cast_unchecked(target_val) }
            } else {
                return None;
            }
        } else {
            return None;
        }
    } else if this.is_proxy() {
        let proxy = unsafe { v8::Local::<v8::Proxy>::cast_unchecked(this) };
        let target_val = proxy.get_target(scope);
        if target_val.is_object() {
            unsafe { v8::Local::<v8::Object>::cast_unchecked(target_val) }
        } else {
            return None;
        }
    } else {
        return None;
    };

    let val = obj.get_private(scope, hidden_key)?;
    extract_node_id_from_external(val)
}

fn extract_node_id_from_external(val: v8::Local<v8::Value>) -> Option<crate::dom::NodeId> {
    let ext = v8::Local::<v8::External>::try_from(val).ok()?;
    let ptr = ext.value() as *const crate::dom::NodeId;
    if ptr.is_null() { return None; }
    Some(unsafe { *ptr })
}

fn get_property_value(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let prop = args.get(0).to_rust_string_lossy(scope);
    let kebab = camel_to_kebab(&prop);

    let Some(node_id) = get_node_id_from_style(scope, args.this()) else {
        rv.set(v8::String::new(scope, "").unwrap().into());
        return;
    };

    let arena = arena_ref(scope);
    let result = if let NodeData::Element(data) = &arena.nodes[node_id].data {
        if let Some(style_str) = data.get_attribute("style") {
            parse_style_attribute(style_str)
                .into_iter()
                .find(|(k, _)| *k == kebab)
                .map(|(_, v)| v)
                .unwrap_or_default()
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let v = v8::String::new(scope, &result).unwrap();
    rv.set(v.into());
}

fn set_property(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let prop = args.get(0).to_rust_string_lossy(scope);
    let value = args.get(1).to_rust_string_lossy(scope);
    let kebab = camel_to_kebab(&prop);

    let Some(node_id) = get_node_id_from_style(scope, args.this()) else { return };

    let arena = arena_mut(scope);
    if let NodeData::Element(data) = &mut arena.nodes[node_id].data {
        let mut props = if let Some(style_str) = data.get_attribute("style") {
            parse_style_attribute(style_str)
        } else {
            vec![]
        };

        // Update or add
        let mut found = false;
        for (k, v) in &mut props {
            if *k == kebab {
                *v = value.clone();
                found = true;
                break;
            }
        }
        if !found {
            props.push((kebab, value));
        }

        let new_style = serialize_style_props(&props);
        data.set_attribute("style", &new_style);
    }
}

fn remove_property(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let prop = args.get(0).to_rust_string_lossy(scope);
    let kebab = camel_to_kebab(&prop);

    let Some(node_id) = get_node_id_from_style(scope, args.this()) else {
        rv.set(v8::String::new(scope, "").unwrap().into());
        return;
    };

    let arena = arena_mut(scope);
    let old_value = if let NodeData::Element(data) = &mut arena.nodes[node_id].data {
        if let Some(style_str) = data.get_attribute("style") {
            let mut props = parse_style_attribute(style_str);
            let old = props.iter()
                .find(|(k, _)| *k == kebab)
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            props.retain(|(k, _)| *k != kebab);
            if props.is_empty() {
                data.remove_attribute("style");
            } else {
                let new_style = serialize_style_props(&props);
                data.set_attribute("style", &new_style);
            }
            old
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let v = v8::String::new(scope, &old_value).unwrap();
    rv.set(v.into());
}

// ─── Style parsing helpers ───────────────────────────────────────────────────

fn parse_style_attribute(style: &str) -> Vec<(String, String)> {
    style
        .split(';')
        .filter_map(|decl| {
            let decl = decl.trim();
            if decl.is_empty() {
                return None;
            }
            let colon = decl.find(':')?;
            let prop = decl[..colon].trim().to_lowercase();
            let value = decl[colon + 1..].trim().to_string();
            Some((prop, value))
        })
        .collect()
}

fn serialize_style_props(props: &[(String, String)]) -> String {
    props
        .iter()
        .map(|(k, v)| format!("{}: {};", k, v))
        .collect::<Vec<_>>()
        .join(" ")
}

fn camel_to_kebab(s: &str) -> String {
    // Special case: cssFloat → float
    if s == "cssFloat" {
        return "float".to_string();
    }
    let mut result = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('-');
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}

fn kebab_to_camel(s: &str) -> String {
    // Special case: float → cssFloat
    if s == "float" {
        return "cssFloat".to_string();
    }
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

#[cfg(test)]
#[path = "style_tests.rs"]
mod tests;

