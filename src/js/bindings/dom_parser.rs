/// DOMParser constructor and parseFromString.

/// Install the DOMParser constructor on the global object.
pub fn install(scope: &mut v8::PinnedRef<v8::HandleScope>, global: v8::Local<v8::Object>) {
    let dp_ctor = v8::Function::new(scope, dom_parser_constructor).unwrap();
    let key = v8::String::new(scope, "DOMParser").unwrap();
    global.set(scope, key.into(), dp_ctor.into());
}

fn dom_parser_constructor(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let obj = v8::Object::new(scope);
    let parse_fn = v8::Function::new(scope, dom_parser_parse_from_string).unwrap();
    let k = v8::String::new(scope, "parseFromString").unwrap();
    obj.set(scope, k.into(), parse_fn.into());
    rv.set(obj.into());
}

fn dom_parser_parse_from_string(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let html = args.get(0).to_rust_string_lossy(scope);
    // Parse the HTML into a temporary arena
    let temp_arena = crate::dom::treesink::parse(&html);
    // Return a minimal document-like object with innerHTML set to the parsed HTML
    let serialized = crate::dom::serialize(&temp_arena);
    // Create a simple object that looks like a Document
    let doc = v8::Object::new(scope);

    // documentElement (the serialized HTML)
    let k = v8::String::new(scope, "documentElement").unwrap();
    let elem = v8::Object::new(scope);
    let k2 = v8::String::new(scope, "innerHTML").unwrap();
    let v2 = v8::String::new(scope, &serialized).unwrap();
    elem.set(scope, k2.into(), v2.into());
    let k2 = v8::String::new(scope, "outerHTML").unwrap();
    elem.set(scope, k2.into(), v2.into());
    doc.set(scope, k.into(), elem.into());

    // body
    let k = v8::String::new(scope, "body").unwrap();
    let body = v8::Object::new(scope);
    let k2 = v8::String::new(scope, "innerHTML").unwrap();
    let v2 = v8::String::new(scope, &serialized).unwrap();
    body.set(scope, k2.into(), v2.into());
    doc.set(scope, k.into(), body.into());

    // querySelector stub
    let qs = v8::Function::new(scope, |scope: &mut v8::PinnedRef<v8::HandleScope>, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        rv.set(v8::null(scope).into());
    }).unwrap();
    let k = v8::String::new(scope, "querySelector").unwrap();
    doc.set(scope, k.into(), qs.into());
    let k = v8::String::new(scope, "querySelectorAll").unwrap();
    let qsa = v8::Function::new(scope, |scope: &mut v8::PinnedRef<v8::HandleScope>, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        rv.set(v8::Array::new(scope, 0).into());
    }).unwrap();
    doc.set(scope, k.into(), qsa.into());

    rv.set(doc.into());
}
