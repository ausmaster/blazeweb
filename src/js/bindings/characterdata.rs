/// CharacterData bindings shared by Text and Comment nodes.
///
/// Ported from Servo's `components/script/dom/characterdata.rs`.
/// Provides: data (get/set), length, substringData, appendData,
/// insertData, deleteData, replaceData, plus ChildNode mixin
/// (before, after, replaceWith, remove) and
/// NonDocumentTypeChildNode mixin (previousElementSibling, nextElementSibling).

use crate::dom::node::NodeData;
use crate::js::templates::{arena_mut, arena_ref, unwrap_node_id, wrap_node};
use super::helpers::{set_accessor, set_accessor_with_setter, set_method};

/// Install CharacterData properties and methods on a prototype.
/// Called by both text::install and comment::install.
pub fn install(scope: &mut v8::HandleScope<()>, proto: &v8::Local<v8::ObjectTemplate>) {
    set_accessor_with_setter(scope, proto, "data", data_getter, data_setter);
    set_accessor(scope, proto, "length", length_getter);
    set_accessor(scope, proto, "nextElementSibling", next_element_sibling);
    set_accessor(scope, proto, "previousElementSibling", prev_element_sibling);
    set_method(scope, proto, "substringData", substring_data);
    set_method(scope, proto, "appendData", append_data);
    set_method(scope, proto, "insertData", insert_data);
    set_method(scope, proto, "deleteData", delete_data);
    set_method(scope, proto, "replaceData", replace_data);
    set_method(scope, proto, "before", before);
    set_method(scope, proto, "after", after);
    set_method(scope, proto, "replaceWith", replace_with);
    set_method(scope, proto, "remove", remove);
}

// ---------------------------------------------------------------------------
// UTF-16 offset helper — ported from Servo's characterdata.rs
// ---------------------------------------------------------------------------

/// Split a UTF-8 string at a position measured in UTF-16 code units.
///
/// Returns `Ok((before, Option<char>, after))`:
/// - `(before, None, after)` — split falls between code points
/// - `(before, Some(ch), after)` — split falls inside an astral character
///   (surrogate pair); `ch` is that character, `before`/`after` exclude it
///
/// Returns `Err(())` if offset is past the end of the string.
fn split_at_utf16_code_unit_offset(s: &str, offset: u32) -> Result<(&str, Option<char>, &str), ()> {
    let mut code_units = 0u32;
    for (i, c) in s.char_indices() {
        if code_units == offset {
            let (a, b) = s.split_at(i);
            return Ok((a, None, b));
        }
        code_units += 1;
        if c > '\u{FFFF}' {
            if code_units == offset {
                // Split inside a surrogate pair.
                return Ok((&s[..i], Some(c), &s[i + c.len_utf8()..]));
            }
            code_units += 1;
        }
    }
    if code_units == offset {
        Ok((s, None, ""))
    } else {
        Err(())
    }
}

/// Count the number of UTF-16 code units in a UTF-8 string.
pub fn utf16_len(s: &str) -> u32 {
    s.encode_utf16().count() as u32
}

// ---------------------------------------------------------------------------
// Accessors
// ---------------------------------------------------------------------------

fn data_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    if let Some(s) = arena.nodes[node_id].data.character_data() {
        let v8_str = v8::String::new(scope, s).unwrap();
        rv.set(v8_str.into());
    }
}

fn data_setter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let val = args.get(0);
    // Per DOM spec: setting data to null sets it to empty string.
    let text = if val.is_null() {
        String::new()
    } else {
        val.to_rust_string_lossy(scope)
    };
    let arena = arena_mut(scope);
    let old_value = arena.nodes[node_id].data.character_data().map(|s| s.to_string());
    if let Some(s) = arena.nodes[node_id].data.character_data_mut() {
        *s = text;
    }
    if let Some(old) = old_value {
        crate::js::mutation_observer::notify_character_data(scope, node_id, &old);
    }
}

fn length_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    if let Some(s) = arena.nodes[node_id].data.character_data() {
        rv.set(v8::Integer::new(scope, utf16_len(s) as i32).into());
    }
}

// ---------------------------------------------------------------------------
// CharacterData methods — ported from Servo
// ---------------------------------------------------------------------------

/// substringData(offset, count)
/// https://dom.spec.whatwg.org/#dom-characterdata-substringdata
fn substring_data(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let offset = args.get(0).uint32_value(scope).unwrap_or(0);
    let count = args.get(1).uint32_value(scope).unwrap_or(0);
    let arena = arena_ref(scope);
    let Some(data) = arena.nodes[node_id].data.character_data() else { return };

    let mut substring = String::new();
    let remaining = match split_at_utf16_code_unit_offset(data, offset) {
        Ok((_, astral, s)) => {
            if astral.is_some() {
                substring.push('\u{FFFD}');
            }
            s
        }
        Err(()) => {
            let msg = v8::String::new(scope, "IndexSizeError: offset is out of range").unwrap();
            let exc = v8::Exception::range_error(scope, msg);
            scope.throw_exception(exc);
            return;
        }
    };
    match split_at_utf16_code_unit_offset(remaining, count) {
        Err(()) => substring.push_str(remaining),
        Ok((s, astral, _)) => {
            substring.push_str(s);
            if astral.is_some() {
                substring.push('\u{FFFD}');
            }
        }
    }
    let v8_str = v8::String::new(scope, &substring).unwrap();
    rv.set(v8_str.into());
}

/// appendData(data)
/// https://dom.spec.whatwg.org/#dom-characterdata-appenddata
fn append_data(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arg = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_mut(scope);
    let old_value = arena.nodes[node_id].data.character_data().map(|s| s.to_string());
    if let Some(s) = arena.nodes[node_id].data.character_data_mut() {
        s.push_str(&arg);
    }
    if let Some(old) = old_value {
        crate::js::mutation_observer::notify_character_data(scope, node_id, &old);
    }
}

/// insertData(offset, data)
/// https://dom.spec.whatwg.org/#dom-characterdata-insertdata
fn insert_data(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    // insertData delegates to replaceData(offset, 0, data)
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let offset = args.get(0).uint32_value(scope).unwrap_or(0);
    let arg = args.get(1).to_rust_string_lossy(scope);
    let arena = arena_mut(scope);
    let Some(data) = arena.nodes[node_id].data.character_data() else { return };
    let old_value = data.to_string();
    match do_replace_data(data, offset, 0, &arg) {
        Ok(new_data) => {
            if let Some(s) = arena.nodes[node_id].data.character_data_mut() {
                *s = new_data;
            }
            crate::js::mutation_observer::notify_character_data(scope, node_id, &old_value);
        }
        Err(msg) => {
            let msg = v8::String::new(scope, msg).unwrap();
            let exc = v8::Exception::range_error(scope, msg);
            scope.throw_exception(exc);
        }
    }
}

/// deleteData(offset, count)
/// https://dom.spec.whatwg.org/#dom-characterdata-deletedata
fn delete_data(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let offset = args.get(0).uint32_value(scope).unwrap_or(0);
    let count = args.get(1).uint32_value(scope).unwrap_or(0);
    let arena = arena_mut(scope);
    let Some(data) = arena.nodes[node_id].data.character_data() else { return };
    let old_value = data.to_string();
    match do_replace_data(data, offset, count, "") {
        Ok(new_data) => {
            if let Some(s) = arena.nodes[node_id].data.character_data_mut() {
                *s = new_data;
            }
            crate::js::mutation_observer::notify_character_data(scope, node_id, &old_value);
        }
        Err(msg) => {
            let msg = v8::String::new(scope, msg).unwrap();
            let exc = v8::Exception::range_error(scope, msg);
            scope.throw_exception(exc);
        }
    }
}

/// replaceData(offset, count, data)
/// https://dom.spec.whatwg.org/#concept-cd-replace
fn replace_data(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let offset = args.get(0).uint32_value(scope).unwrap_or(0);
    let count = args.get(1).uint32_value(scope).unwrap_or(0);
    let arg = args.get(2).to_rust_string_lossy(scope);
    let arena = arena_mut(scope);
    let Some(data) = arena.nodes[node_id].data.character_data() else { return };
    let old_value = data.to_string();
    match do_replace_data(data, offset, count, &arg) {
        Ok(new_data) => {
            if let Some(s) = arena.nodes[node_id].data.character_data_mut() {
                *s = new_data;
            }
            crate::js::mutation_observer::notify_character_data(scope, node_id, &old_value);
        }
        Err(msg) => {
            let msg = v8::String::new(scope, msg).unwrap();
            let exc = v8::Exception::range_error(scope, msg);
            scope.throw_exception(exc);
        }
    }
}

/// Core replaceData algorithm ported from Servo.
/// Returns the new data string or an error message.
fn do_replace_data(data: &str, offset: u32, count: u32, arg: &str) -> Result<String, &'static str> {
    let prefix;
    let replacement_before;
    let remaining;
    match split_at_utf16_code_unit_offset(data, offset) {
        Ok((p, astral, r)) => {
            prefix = p;
            replacement_before = if astral.is_some() { "\u{FFFD}" } else { "" };
            remaining = r;
        }
        Err(()) => return Err("IndexSizeError: offset is out of range"),
    }
    let replacement_after;
    let suffix;
    match split_at_utf16_code_unit_offset(remaining, count) {
        Err(()) => {
            replacement_after = "";
            suffix = "";
        }
        Ok((_, astral, s)) => {
            replacement_after = if astral.is_some() { "\u{FFFD}" } else { "" };
            suffix = s;
        }
    }
    let mut new_data = String::with_capacity(
        prefix.len() + replacement_before.len() + arg.len() + replacement_after.len() + suffix.len(),
    );
    new_data.push_str(prefix);
    new_data.push_str(replacement_before);
    new_data.push_str(arg);
    new_data.push_str(replacement_after);
    new_data.push_str(suffix);
    Ok(new_data)
}

// ---------------------------------------------------------------------------
// ChildNode mixin: before(), after(), replaceWith(), remove()
// https://dom.spec.whatwg.org/#interface-childnode
// ---------------------------------------------------------------------------

fn before(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_mut(scope);
    let parent = match arena.nodes[node_id].parent {
        Some(p) => p,
        None => return, // No parent → no-op per spec
    };
    // Collect arguments as node IDs or new text nodes
    let new_nodes = collect_node_or_string_args(scope, &args, arena);
    let arena = arena_mut(scope);
    for nid in new_nodes {
        arena.insert_before(node_id, nid);
    }
    let _ = parent; // used only to check existence
}

fn after(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_mut(scope);
    let parent = match arena.nodes[node_id].parent {
        Some(p) => p,
        None => return, // No parent → no-op per spec
    };
    let next_sibling = arena.nodes[node_id].next_sibling;
    let new_nodes = collect_node_or_string_args(scope, &args, arena);
    let arena = arena_mut(scope);
    for nid in new_nodes {
        if let Some(ref_node) = next_sibling {
            arena.insert_before(ref_node, nid);
        } else {
            arena.append_child(parent, nid);
        }
    }
}

fn replace_with(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_mut(scope);
    let parent = match arena.nodes[node_id].parent {
        Some(p) => p,
        None => return, // No parent → no-op per spec
    };
    let next_sibling = arena.nodes[node_id].next_sibling;
    arena.detach(node_id);
    let new_nodes = collect_node_or_string_args(scope, &args, arena);
    let arena = arena_mut(scope);
    for nid in new_nodes {
        if let Some(ref_node) = next_sibling {
            arena.insert_before(ref_node, nid);
        } else {
            arena.append_child(parent, nid);
        }
    }
}

fn remove(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_mut(scope);
    arena.detach(node_id);
}

/// Collect variadic arguments as node IDs. Strings become new Text nodes.
fn collect_node_or_string_args(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    arena: &mut crate::dom::Arena,
) -> Vec<crate::dom::NodeId> {
    let mut result = Vec::new();
    for i in 0..args.length() {
        let val = args.get(i);
        if val.is_object() {
            let obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(val) };
            if let Some(nid) = unwrap_node_id(scope, obj) {
                result.push(nid);
                continue;
            }
        }
        // String argument → create text node
        let text = val.to_rust_string_lossy(scope);
        let nid = arena.new_node(NodeData::Text(text));
        result.push(nid);
    }
    result
}

// ---------------------------------------------------------------------------
// NonDocumentTypeChildNode mixin: previousElementSibling, nextElementSibling
// https://dom.spec.whatwg.org/#interface-nondocumenttypechildnode
// ---------------------------------------------------------------------------

fn prev_element_sibling(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let mut current = arena.nodes[node_id].prev_sibling;
    while let Some(sib) = current {
        if matches!(&arena.nodes[sib].data, NodeData::Element(_)) {
            let obj = wrap_node(scope, sib);
            rv.set(obj.into());
            return;
        }
        current = arena.nodes[sib].prev_sibling;
    }
    rv.set(v8::null(scope).into());
}

fn next_element_sibling(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    let mut current = arena.nodes[node_id].next_sibling;
    while let Some(sib) = current {
        if matches!(&arena.nodes[sib].data, NodeData::Element(_)) {
            let obj = wrap_node(scope, sib);
            rv.set(obj.into());
            return;
        }
        current = arena.nodes[sib].next_sibling;
    }
    rv.set(v8::null(scope).into());
}
