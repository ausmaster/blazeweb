/// Text (CharacterData) prototype bindings.
///
/// Properties: data (get/set), length (readonly).

use crate::dom::node::NodeData;
use crate::js::templates::{arena_mut, arena_ref, unwrap_node_id};

pub fn install(scope: &mut v8::HandleScope<()>, proto: &v8::Local<v8::ObjectTemplate>) {
    let key = v8::String::new(scope, "data").unwrap();
    let getter_ft = v8::FunctionTemplate::new(scope, data_getter);
    let setter_ft = v8::FunctionTemplate::new(scope, data_setter);
    proto.set_accessor_property(key.into(), Some(getter_ft), Some(setter_ft), v8::PropertyAttribute::NONE);

    let key = v8::String::new(scope, "wholeText").unwrap();
    let getter_ft = v8::FunctionTemplate::new(scope, data_getter);
    proto.set_accessor_property(key.into(), Some(getter_ft), None, v8::PropertyAttribute::NONE);

    let key = v8::String::new(scope, "length").unwrap();
    let getter_ft = v8::FunctionTemplate::new(scope, length_getter);
    proto.set_accessor_property(key.into(), Some(getter_ft), None, v8::PropertyAttribute::NONE);
}

fn data_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    if let NodeData::Text(s) = &arena.nodes[node_id].data {
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
    let text = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_mut(scope);
    if let NodeData::Text(s) = &mut arena.nodes[node_id].data {
        *s = text;
    }
}

fn length_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    if let NodeData::Text(s) = &arena.nodes[node_id].data {
        rv.set(v8::Integer::new(scope, s.len() as i32).into());
    }
}
