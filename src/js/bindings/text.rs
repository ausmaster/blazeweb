/// Text prototype bindings.
///
/// Inherits CharacterData (data, length, substringData, appendData, etc.)
/// and adds Text-specific: splitText, wholeText.
///
/// Ported from Servo's `components/script/dom/text.rs` and `characterdata.rs`.

use crate::dom::node::NodeData;
use crate::js::templates::{arena_mut, arena_ref, unwrap_node_id, wrap_node};
use super::helpers::{set_accessor, set_method};

pub fn install(scope: &mut v8::HandleScope<()>, proto: &v8::Local<v8::ObjectTemplate>) {
    // CharacterData shared properties and methods
    super::characterdata::install(scope, proto);

    // Text-specific
    set_accessor(scope, proto, "wholeText", whole_text_getter);
    set_method(scope, proto, "splitText", split_text);
}

/// wholeText — concatenation of contiguous Text sibling data.
/// https://dom.spec.whatwg.org/#dom-text-wholetext
fn whole_text_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);

    // Walk backwards to find first contiguous Text node.
    let mut first = node_id;
    while let Some(prev) = arena.nodes[first].prev_sibling {
        if matches!(&arena.nodes[prev].data, NodeData::Text(_)) {
            first = prev;
        } else {
            break;
        }
    }

    // Walk forward collecting text data.
    let mut result = String::new();
    let mut current = Some(first);
    while let Some(nid) = current {
        if let NodeData::Text(s) = &arena.nodes[nid].data {
            result.push_str(s);
            current = arena.nodes[nid].next_sibling;
        } else {
            break;
        }
    }

    let v8_str = v8::String::new(scope, &result).unwrap();
    rv.set(v8_str.into());
}

/// splitText(offset) — split this Text node at UTF-16 code unit offset.
/// https://dom.spec.whatwg.org/#concept-text-split
///
/// Ported from Servo's text.rs SplitText implementation.
fn split_text(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let offset = args.get(0).uint32_value(scope).unwrap_or(0);

    let arena = arena_mut(scope);

    // Step 1: Get length.
    let length = {
        let Some(data) = arena.nodes[node_id].data.character_data() else { return };
        super::characterdata::utf16_len(data)
    };

    // Step 2: If offset > length, throw IndexSizeError.
    if offset > length {
        let msg = v8::String::new(scope, "IndexSizeError: offset is out of range").unwrap();
        let exc = v8::Exception::range_error(scope, msg);
        scope.throw_exception(exc);
        return;
    }

    // Step 3: count = length - offset
    let count = length - offset;

    // Step 4: Get new_data = substringData(offset, count)
    let new_data = {
        let data = arena.nodes[node_id].data.character_data().unwrap();
        substring_utf16(data, offset, count)
    };

    // Step 5: Create new text node with new_data
    let new_node_id = arena.new_node(NodeData::Text(new_data));

    // Step 6: If parent exists, insert new node after this node
    let parent = arena.nodes[node_id].parent;
    if let Some(parent_id) = parent {
        let next_sibling = arena.nodes[node_id].next_sibling;
        if let Some(ref_node) = next_sibling {
            arena.insert_before(ref_node, new_node_id);
        } else {
            arena.append_child(parent_id, new_node_id);
        }
    }

    // Step 8: Delete data from offset in original node (truncate)
    {
        let data = arena.nodes[node_id].data.character_data().unwrap();
        let truncated = prefix_utf16(data, offset);
        if let Some(s) = arena.nodes[node_id].data.character_data_mut() {
            *s = truncated;
        }
    }

    // Step 9: Return new node
    let obj = wrap_node(scope, new_node_id);
    rv.set(obj.into());
}

/// Extract substring starting at UTF-16 offset for `count` UTF-16 code units.
fn substring_utf16(s: &str, offset: u32, count: u32) -> String {
    let units: Vec<u16> = s.encode_utf16()
        .skip(offset as usize)
        .take(count as usize)
        .collect();
    String::from_utf16_lossy(&units)
}

/// Get the prefix of a string up to a UTF-16 code unit offset.
fn prefix_utf16(s: &str, offset: u32) -> String {
    let units: Vec<u16> = s.encode_utf16().take(offset as usize).collect();
    String::from_utf16_lossy(&units)
}
