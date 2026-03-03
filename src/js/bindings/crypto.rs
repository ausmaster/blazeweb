/// Crypto API: getRandomValues, randomUUID, and subtle stub.

/// Install the `crypto` object on the global object.
pub fn install(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    let crypto = v8::Object::new(scope);
    let grv = v8::Function::new(scope, crypto_get_random_values).unwrap();
    let k = v8::String::new(scope, "getRandomValues").unwrap();
    crypto.set(scope, k.into(), grv.into());
    let ruuid = v8::Function::new(scope, crypto_random_uuid).unwrap();
    let k = v8::String::new(scope, "randomUUID").unwrap();
    crypto.set(scope, k.into(), ruuid.into());
    // subtle stub (empty object)
    let subtle = v8::Object::new(scope);
    let k = v8::String::new(scope, "subtle").unwrap();
    crypto.set(scope, k.into(), subtle.into());
    let key = v8::String::new(scope, "crypto").unwrap();
    global.set(scope, key.into(), crypto.into());
}

fn crypto_get_random_values(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let arr = args.get(0);
    if let Ok(buf) = v8::Local::<v8::ArrayBufferView>::try_from(arr) {
        let len = buf.byte_length();
        let mut bytes = vec![0u8; len];
        // Simple pseudo-random fill
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = ((i * 1103515245 + 12345) >> 16) as u8;
        }
        let backing = buf.buffer(scope).unwrap();
        let store = backing.get_backing_store();
        let offset = buf.byte_offset();
        for (i, &byte) in bytes.iter().enumerate() {
            store[offset + i].set(byte);
        }
    }
    rv.set(arr);
}

fn crypto_random_uuid(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    // Generate a v4 UUID using a simple counter-based pseudo-random
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0x1234567890abcdef);
    let val = COUNTER.fetch_add(0x6a09e667f3bcc908, std::sync::atomic::Ordering::Relaxed);
    let uuid = format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (val >> 32) as u32,
        (val >> 16) as u16,
        (val & 0xFFF) as u16,
        0x8000 | ((val >> 48) as u16 & 0x3FFF),
        val & 0xFFFFFFFFFFFF
    );
    let v = v8::String::new(scope, &uuid).unwrap();
    rv.set(v.into());
}
