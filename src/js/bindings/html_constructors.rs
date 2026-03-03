/// HTML element constructors: Image, Audio, Option.

use crate::dom::node::ElementData;
use crate::js::templates::{arena_mut, wrap_node};

/// Install Image, Audio, Option constructors on the global object.
pub fn install(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    let image_ctor = v8::Function::new(scope, image_constructor).unwrap();
    let key = v8::String::new(scope, "Image").unwrap();
    global.set(scope, key.into(), image_ctor.into());

    let audio_ctor = v8::Function::new(scope, audio_constructor).unwrap();
    let key = v8::String::new(scope, "Audio").unwrap();
    global.set(scope, key.into(), audio_ctor.into());

    let option_ctor = v8::Function::new(scope, option_constructor).unwrap();
    let key = v8::String::new(scope, "Option").unwrap();
    global.set(scope, key.into(), option_ctor.into());
}

fn image_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let arena = arena_mut(scope);
    let name = markup5ever::QualName::new(None, markup5ever::ns!(html), "img".into());
    let mut attrs = vec![];
    if args.length() > 0 && !args.get(0).is_undefined() {
        let w = args.get(0).to_rust_string_lossy(scope);
        attrs.push(markup5ever::Attribute { name: markup5ever::QualName::new(None, markup5ever::ns!(), "width".into()), value: w.into() });
    }
    if args.length() > 1 && !args.get(1).is_undefined() {
        let h = args.get(1).to_rust_string_lossy(scope);
        attrs.push(markup5ever::Attribute { name: markup5ever::QualName::new(None, markup5ever::ns!(), "height".into()), value: h.into() });
    }
    let node_id = arena.new_node(crate::dom::node::NodeData::Element(ElementData::new(name, attrs)));
    rv.set(wrap_node(scope, node_id).into());
}

fn audio_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let arena = arena_mut(scope);
    let name = markup5ever::QualName::new(None, markup5ever::ns!(html), "audio".into());
    let mut attrs = vec![];
    if args.length() > 0 && !args.get(0).is_undefined() {
        let src = args.get(0).to_rust_string_lossy(scope);
        attrs.push(markup5ever::Attribute { name: markup5ever::QualName::new(None, markup5ever::ns!(), "src".into()), value: src.into() });
    }
    let node_id = arena.new_node(crate::dom::node::NodeData::Element(ElementData::new(name, attrs)));
    rv.set(wrap_node(scope, node_id).into());
}

fn option_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let arena = arena_mut(scope);
    let name = markup5ever::QualName::new(None, markup5ever::ns!(html), "option".into());
    let mut attrs = vec![];
    if args.length() > 1 && !args.get(1).is_undefined() {
        let value = args.get(1).to_rust_string_lossy(scope);
        attrs.push(markup5ever::Attribute { name: markup5ever::QualName::new(None, markup5ever::ns!(), "value".into()), value: value.into() });
    }
    if args.length() > 3 && args.get(3).boolean_value(scope) {
        attrs.push(markup5ever::Attribute { name: markup5ever::QualName::new(None, markup5ever::ns!(), "selected".into()), value: "".into() });
    }
    let node_id = arena.new_node(crate::dom::node::NodeData::Element(ElementData::new(name, attrs)));
    if args.length() > 0 && !args.get(0).is_undefined() {
        let text = args.get(0).to_rust_string_lossy(scope);
        let text_node = arena.new_node(crate::dom::node::NodeData::Text(text));
        arena.append_child(node_id, text_node);
    }
    rv.set(wrap_node(scope, node_id).into());
}
