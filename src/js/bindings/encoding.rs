/// TextEncoder and TextDecoder constructors.

/// Install TextEncoder and TextDecoder constructors on the global object.
pub fn install(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    let te_ctor = v8::Function::new(scope, text_encoder_constructor).unwrap();
    let key = v8::String::new(scope, "TextEncoder").unwrap();
    global.set(scope, key.into(), te_ctor.into());

    let td_ctor = v8::Function::new(scope, text_decoder_constructor).unwrap();
    let key = v8::String::new(scope, "TextDecoder").unwrap();
    global.set(scope, key.into(), td_ctor.into());
}

fn text_encoder_constructor(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let obj = v8::Object::new(scope);
    let k = v8::String::new(scope, "encoding").unwrap();
    let v = v8::String::new(scope, "utf-8").unwrap();
    obj.set(scope, k.into(), v.into());

    let encode_fn = v8::Function::new(scope, text_encoder_encode).unwrap();
    let k = v8::String::new(scope, "encode").unwrap();
    obj.set(scope, k.into(), encode_fn.into());

    let encode_into_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let result = v8::Object::new(scope);
        let input = args.get(0).to_rust_string_lossy(scope);
        let k = v8::String::new(scope, "read").unwrap();
        let v = v8::Integer::new(scope, input.len() as i32);
        result.set(scope, k.into(), v.into());
        let k = v8::String::new(scope, "written").unwrap();
        let v = v8::Integer::new(scope, input.len() as i32);
        result.set(scope, k.into(), v.into());
        rv.set(result.into());
    }).unwrap();
    let k = v8::String::new(scope, "encodeInto").unwrap();
    obj.set(scope, k.into(), encode_into_fn.into());

    rv.set(obj.into());
}

fn text_encoder_encode(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let input = args.get(0).to_rust_string_lossy(scope);
    let bytes = input.as_bytes();
    let backing = v8::ArrayBuffer::new(scope, bytes.len());
    let store = backing.get_backing_store();
    for (i, &b) in bytes.iter().enumerate() {
        store[i].set(b);
    }
    let uint8 = v8::Uint8Array::new(scope, backing, 0, bytes.len()).unwrap();
    rv.set(uint8.into());
}

fn text_decoder_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let obj = v8::Object::new(scope);
    let encoding = if args.length() > 0 && !args.get(0).is_undefined() {
        args.get(0).to_rust_string_lossy(scope).to_ascii_lowercase()
    } else {
        "utf-8".to_string()
    };
    let k = v8::String::new(scope, "encoding").unwrap();
    let v = v8::String::new(scope, &encoding).unwrap();
    obj.set(scope, k.into(), v.into());

    let decode_fn = v8::Function::new(scope, text_decoder_decode).unwrap();
    let k = v8::String::new(scope, "decode").unwrap();
    obj.set(scope, k.into(), decode_fn.into());

    rv.set(obj.into());
}

fn text_decoder_decode(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let input = args.get(0);
    if input.is_undefined() || input.is_null() {
        let v = v8::String::new(scope, "").unwrap();
        rv.set(v.into());
        return;
    }

    if let Ok(view) = v8::Local::<v8::ArrayBufferView>::try_from(input) {
        let len = view.byte_length();
        let offset = view.byte_offset();
        let backing = view.buffer(scope).unwrap();
        let store = backing.get_backing_store();
        let mut bytes = vec![0u8; len];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = store[offset + i].get();
        }
        let decoded = String::from_utf8_lossy(&bytes);
        let v = v8::String::new(scope, &decoded).unwrap();
        rv.set(v.into());
    } else if let Ok(buf) = v8::Local::<v8::ArrayBuffer>::try_from(input) {
        let store = buf.get_backing_store();
        let len = buf.byte_length();
        let mut bytes = vec![0u8; len];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = store[i].get();
        }
        let decoded = String::from_utf8_lossy(&bytes);
        let v = v8::String::new(scope, &decoded).unwrap();
        rv.set(v.into());
    } else {
        let v = v8::String::new(scope, "").unwrap();
        rv.set(v.into());
    }
}
