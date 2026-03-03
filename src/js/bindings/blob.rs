/// Blob API — immutable raw data container.

/// Install the Blob constructor on the global object.
pub fn install(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    let blob_ctor = v8::Function::new(scope, blob_constructor).unwrap();
    let key = v8::String::new(scope, "Blob").unwrap();
    global.set(scope, key.into(), blob_ctor.into());
}

fn blob_constructor(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let obj = v8::Object::new(scope);
    let mut content = String::new();
    if args.length() > 0 && args.get(0).is_array() {
        let parts = unsafe { v8::Local::<v8::Array>::cast_unchecked(args.get(0)) };
        for i in 0..parts.length() {
            if let Some(part) = parts.get_index(scope, i) { content.push_str(&part.to_rust_string_lossy(scope)); }
        }
    }
    let mime_type = if args.length() > 1 && args.get(1).is_object() {
        let opts = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        let k = v8::String::new(scope, "type").unwrap();
        opts.get(scope, k.into()).map(|v| v.to_rust_string_lossy(scope)).unwrap_or_default()
    } else { String::new() };

    let k = v8::String::new(scope, "size").unwrap();
    let val = v8::Integer::new(scope, content.len() as i32);
    obj.set(scope, k.into(), val.into());
    let k = v8::String::new(scope, "type").unwrap();
    let v = v8::String::new(scope, &mime_type).unwrap();
    obj.set(scope, k.into(), v.into());

    let pk = v8::String::new(scope, "__blobContent").unwrap();
    let hidden_key = v8::Private::for_api(scope, Some(pk));
    let v = v8::String::new(scope, &content).unwrap();
    obj.set_private(scope, hidden_key, v.into());

    let text_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let pk = v8::String::new(scope, "__blobContent").unwrap();
        let hidden_key = v8::Private::for_api(scope, Some(pk));
        let content = args.this().get_private(scope, hidden_key).map(|v| v.to_rust_string_lossy(scope)).unwrap_or_default();
        let resolver = v8::PromiseResolver::new(scope).unwrap();
        let v = v8::String::new(scope, &content).unwrap();
        resolver.resolve(scope, v.into());
        rv.set(resolver.get_promise(scope).into());
    }).unwrap();
    let k = v8::String::new(scope, "text").unwrap();
    obj.set(scope, k.into(), text_fn.into());

    let ab_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let pk = v8::String::new(scope, "__blobContent").unwrap();
        let hidden_key = v8::Private::for_api(scope, Some(pk));
        let content = args.this().get_private(scope, hidden_key).map(|v| v.to_rust_string_lossy(scope)).unwrap_or_default();
        let bytes = content.as_bytes();
        let buf = v8::ArrayBuffer::new(scope, bytes.len());
        let store = buf.get_backing_store();
        for (i, &b) in bytes.iter().enumerate() { store[i].set(b); }
        let resolver = v8::PromiseResolver::new(scope).unwrap();
        resolver.resolve(scope, buf.into());
        rv.set(resolver.get_promise(scope).into());
    }).unwrap();
    let k = v8::String::new(scope, "arrayBuffer").unwrap();
    obj.set(scope, k.into(), ab_fn.into());

    let noop = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        rv.set(v8::Object::new(scope).into());
    }).unwrap();
    let k = v8::String::new(scope, "slice").unwrap();
    obj.set(scope, k.into(), noop.into());
    let k = v8::String::new(scope, "stream").unwrap();
    obj.set(scope, k.into(), noop.into());

    rv.set(obj.into());
}
