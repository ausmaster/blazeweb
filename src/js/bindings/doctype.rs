/// DocumentType prototype bindings.
///
/// Installed on the DocumentType FunctionTemplate prototype during
/// create_dom_templates(). Provides name, publicId, systemId accessors.

use crate::dom::node::NodeData;
use crate::js::templates::{arena_ref, unwrap_node_id};
use super::helpers::set_accessor;

pub fn install(scope: &mut v8::PinnedRef<v8::HandleScope<()>>, proto: &v8::Local<v8::ObjectTemplate>) {
    set_accessor(scope, proto, "name", name_getter);
    set_accessor(scope, proto, "publicId", public_id_getter);
    set_accessor(scope, proto, "systemId", system_id_getter);
}

fn name_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    if let NodeData::Doctype { name, .. } = &arena.nodes[node_id].data {
        let v = v8::String::new(scope, name).unwrap();
        rv.set(v.into());
    } else {
        let v = v8::String::new(scope, "").unwrap();
        rv.set(v.into());
    }
}

fn public_id_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    if let NodeData::Doctype { public_id, .. } = &arena.nodes[node_id].data {
        let v = v8::String::new(scope, public_id).unwrap();
        rv.set(v.into());
    } else {
        let v = v8::String::new(scope, "").unwrap();
        rv.set(v.into());
    }
}

fn system_id_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);
    if let NodeData::Doctype { system_id, .. } = &arena.nodes[node_id].data {
        let v = v8::String::new(scope, system_id).unwrap();
        rv.set(v.into());
    } else {
        let v = v8::String::new(scope, "").unwrap();
        rv.set(v.into());
    }
}
