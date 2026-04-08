/// URL, URLSearchParams, and base64 (atob/btoa) bindings.
///
/// Registers URL constructor, URLSearchParams constructor, atob, and btoa
/// on the global object.

/// Install URL, URLSearchParams, atob, and btoa on the global object.
pub fn install(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    // URL constructor
    let url_ctor = v8::Function::new(scope, url_constructor).unwrap();
    let key = v8::String::new(scope, "URL").unwrap();
    global.set(scope, key.into(), url_ctor.into());

    // URLSearchParams constructor
    let usp_ctor = v8::Function::new(scope, url_search_params_constructor).unwrap();
    let key = v8::String::new(scope, "URLSearchParams").unwrap();
    global.set(scope, key.into(), usp_ctor.into());

    // atob
    let atob_fn = v8::Function::new(scope, atob).unwrap();
    let key = v8::String::new(scope, "atob").unwrap();
    global.set(scope, key.into(), atob_fn.into());

    // btoa
    let btoa_fn = v8::Function::new(scope, btoa).unwrap();
    let key = v8::String::new(scope, "btoa").unwrap();
    global.set(scope, key.into(), btoa_fn.into());
}

// ─── URL constructor ─────────────────────────────────────────────────────────

fn url_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let url_str = args.get(0).to_rust_string_lossy(scope);
    let base_str = if args.length() > 1 && !args.get(1).is_undefined() {
        Some(args.get(1).to_rust_string_lossy(scope))
    } else {
        None
    };

    let resolved = if let Some(base) = &base_str {
        if let Ok(base_url) = reqwest::Url::parse(base) {
            base_url.join(&url_str).map(|u| u.to_string()).unwrap_or(url_str.clone())
        } else {
            url_str.clone()
        }
    } else {
        url_str.clone()
    };

    let obj = v8::Object::new(scope);
    // Parse and set URL parts
    if let Ok(url) = reqwest::Url::parse(&resolved) {
        let set_str = |scope: &mut v8::HandleScope, obj: v8::Local<v8::Object>, key: &str, val: &str| {
            let k = v8::String::new(scope, key).unwrap();
            let v = v8::String::new(scope, val).unwrap();
            obj.set(scope, k.into(), v.into());
        };
        set_str(scope, obj, "href", url.as_str());
        set_str(scope, obj, "protocol", &format!("{}:", url.scheme()));
        set_str(scope, obj, "hostname", url.host_str().unwrap_or(""));
        set_str(scope, obj, "port", &url.port().map(|p| p.to_string()).unwrap_or_default());
        set_str(scope, obj, "pathname", url.path());
        set_str(scope, obj, "search", &url.query().map(|q| format!("?{}", q)).unwrap_or_default());
        set_str(scope, obj, "hash", &url.fragment().map(|f| format!("#{}", f)).unwrap_or_default());
        let host = if let Some(port) = url.port() {
            format!("{}:{}", url.host_str().unwrap_or(""), port)
        } else {
            url.host_str().unwrap_or("").to_string()
        };
        set_str(scope, obj, "host", &host);
        set_str(scope, obj, "origin", &format!("{}://{}", url.scheme(), host));

        // toString and toJSON — read href back from the object
        let to_string = v8::Function::new(scope, |scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
            let this = args.this();
            let k = v8::String::new(scope, "href").unwrap();
            if let Some(val) = this.get(scope, k.into()) {
                rv.set(val);
            }
        }).unwrap();
        let k = v8::String::new(scope, "toString").unwrap();
        obj.set(scope, k.into(), to_string.into());
        let k = v8::String::new(scope, "toJSON").unwrap();
        obj.set(scope, k.into(), to_string.into());
    }

    rv.set(obj.into());
}

// ─── Base64 helpers ──────────────────────────────────────────────────────────

const B64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut result = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(B64_CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(B64_CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(B64_CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(B64_CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn base64_decode(input: &str) -> Option<String> {
    let input = input.trim();
    if input.is_empty() {
        return Some(String::new());
    }
    let mut chars: Vec<u8> = input.bytes().filter(|&b| b != b'\n' && b != b'\r' && b != b' ' && b != b'\t').collect();

    // Auto-pad to multiple of 4 with '=' (browsers accept unpadded base64)
    while chars.len() % 4 != 0 {
        chars.push(b'=');
    }

    let mut bytes = Vec::new();
    for chunk in chars.chunks(4) {
        let vals: Vec<Option<u8>> = chunk.iter().map(|&c| b64_decode_char(c)).collect();
        let a = vals[0]? as u32;
        let b = vals[1]? as u32;
        bytes.push(((a << 2) | (b >> 4)) as u8);
        if chunk[2] != b'=' {
            let c = vals[2]? as u32;
            bytes.push((((b & 0xF) << 4) | (c >> 2)) as u8);
            if chunk[3] != b'=' {
                let d = vals[3]? as u32;
                bytes.push((((c & 0x3) << 6) | d) as u8);
            }
        }
    }
    // Return Latin-1 string: each byte maps directly to a char code (0-255).
    // This matches the atob spec which returns a "binary string" where
    // charCodeAt(i) == byte[i], not a UTF-8 string.
    Some(bytes.iter().map(|&b| b as char).collect::<String>())
}

fn b64_decode_char(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        b'=' => Some(0),
        _ => None,
    }
}

fn atob(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let input = args.get(0).to_rust_string_lossy(scope);
    // Simple base64 decode
    match base64_decode(&input) {
        Some(decoded) => {
            let v = v8::String::new(scope, &decoded).unwrap();
            rv.set(v.into());
        }
        None => {
            let msg = v8::String::new(scope, "Invalid base64 string").unwrap();
            let exc = v8::Exception::error(scope, msg);
            scope.throw_exception(exc);
        }
    }
}

fn btoa(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let input = args.get(0).to_rust_string_lossy(scope);
    let encoded = base64_encode(&input);
    let v = v8::String::new(scope, &encoded).unwrap();
    rv.set(v.into());
}

// ─── URLSearchParams ─────────────────────────────────────────────────────────

fn url_search_params_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let obj = v8::Object::new(scope);

    // Internal storage: use a JS Map for simplicity — but we'll use an ordered array of pairs
    // stored in a private field. For simplicity, parse init string and store as JS array.
    let pairs = v8::Array::new(scope, 0);
    let pk_name = v8::String::new(scope, "__pairs").unwrap();
    let hidden_key = v8::Private::for_api(scope, Some(pk_name));
    obj.set_private(scope, hidden_key, pairs.into());

    // Parse init if provided
    if args.length() > 0 {
        let init = args.get(0);
        if init.is_string() {
            let init_str = init.to_rust_string_lossy(scope);
            let query = init_str.strip_prefix('?').unwrap_or(&init_str);
            let mut idx = 0u32;
            for part in query.split('&') {
                if part.is_empty() { continue; }
                let (k, v) = part.split_once('=').unwrap_or((part, ""));
                let decoded_k = url_decode(k);
                let decoded_v = url_decode(v);
                let pair = v8::Array::new(scope, 2);
                let ks = v8::String::new(scope, &decoded_k).unwrap();
                let vs = v8::String::new(scope, &decoded_v).unwrap();
                pair.set_index(scope, 0, ks.into());
                pair.set_index(scope, 1, vs.into());
                pairs.set_index(scope, idx, pair.into());
                idx += 1;
            }
        }
    }

    // Methods
    macro_rules! set_usp_method {
        ($scope:expr, $obj:expr, $name:expr, $cb:ident) => {{
            let func = v8::Function::new($scope, $cb).unwrap();
            let k = v8::String::new($scope, $name).unwrap();
            $obj.set($scope, k.into(), func.into());
        }};
    }
    set_usp_method!(scope, obj, "get", usp_get);
    set_usp_method!(scope, obj, "set", usp_set);
    set_usp_method!(scope, obj, "has", usp_has);
    set_usp_method!(scope, obj, "delete", usp_delete);
    set_usp_method!(scope, obj, "append", usp_append);
    set_usp_method!(scope, obj, "toString", usp_to_string);
    set_usp_method!(scope, obj, "forEach", usp_for_each);
    set_usp_method!(scope, obj, "entries", usp_entries);
    set_usp_method!(scope, obj, "keys", usp_keys);
    set_usp_method!(scope, obj, "values", usp_values);
    set_usp_method!(scope, obj, "getAll", usp_get_all);

    // size getter (computed property — approximate with a data property for now)
    let k = v8::String::new(scope, "size").unwrap();
    let v = v8::Integer::new(scope, pairs.length() as i32);
    obj.set(scope, k.into(), v.into());

    rv.set(obj.into());
}

fn url_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.bytes();
    while let Some(b) = chars.next() {
        if b == b'+' {
            result.push(' ');
        } else if b == b'%' {
            let h = chars.next().unwrap_or(0);
            let l = chars.next().unwrap_or(0);
            let decoded = (hex_val(h) << 4) | hex_val(l);
            result.push(decoded as char);
        } else {
            result.push(b as char);
        }
    }
    result
}

fn hex_val(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0,
    }
}

fn url_encode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => result.push(b as char),
            b' ' => result.push('+'),
            _ => {
                result.push('%');
                result.push(char::from(b"0123456789ABCDEF"[(b >> 4) as usize]));
                result.push(char::from(b"0123456789ABCDEF"[(b & 0xF) as usize]));
            }
        }
    }
    result
}

fn usp_get_pairs<'s>(scope: &mut v8::HandleScope<'s>, this: v8::Local<v8::Object>) -> Option<v8::Local<'s, v8::Array>> {
    let pk_name = v8::String::new(scope, "__pairs").unwrap();
    let hidden_key = v8::Private::for_api(scope, Some(pk_name));
    let val = this.get_private(scope, hidden_key)?;
    v8::Local::<v8::Array>::try_from(val).ok()
}

fn usp_get(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let key = args.get(0).to_rust_string_lossy(scope);
    let Some(pairs) = usp_get_pairs(scope, args.this()) else { return };
    for i in 0..pairs.length() {
        if let Some(pair) = pairs.get_index(scope, i) {
            let pair = unsafe { v8::Local::<v8::Array>::cast_unchecked(pair) };
            if let Some(k) = pair.get_index(scope, 0) {
                if k.to_rust_string_lossy(scope) == key {
                    if let Some(v) = pair.get_index(scope, 1) {
                        rv.set(v);
                        return;
                    }
                }
            }
        }
    }
    rv.set(v8::null(scope).into());
}

fn usp_get_all(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let key = args.get(0).to_rust_string_lossy(scope);
    let Some(pairs) = usp_get_pairs(scope, args.this()) else { return };
    let result = v8::Array::new(scope, 0);
    let mut idx = 0u32;
    for i in 0..pairs.length() {
        if let Some(pair) = pairs.get_index(scope, i) {
            let pair = unsafe { v8::Local::<v8::Array>::cast_unchecked(pair) };
            if let Some(k) = pair.get_index(scope, 0) {
                if k.to_rust_string_lossy(scope) == key {
                    if let Some(v) = pair.get_index(scope, 1) {
                        result.set_index(scope, idx, v);
                        idx += 1;
                    }
                }
            }
        }
    }
    rv.set(result.into());
}

fn usp_set(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue) {
    let key = args.get(0).to_rust_string_lossy(scope);
    let val = args.get(1);
    let Some(pairs) = usp_get_pairs(scope, args.this()) else { return };
    // Remove existing entries with this key, then add one
    let mut found = false;
    for i in 0..pairs.length() {
        if let Some(pair) = pairs.get_index(scope, i) {
            let pair = unsafe { v8::Local::<v8::Array>::cast_unchecked(pair) };
            if let Some(k) = pair.get_index(scope, 0) {
                if k.to_rust_string_lossy(scope) == key {
                    if !found {
                        pair.set_index(scope, 1, val);
                        found = true;
                    }
                }
            }
        }
    }
    if !found {
        let pair = v8::Array::new(scope, 2);
        let ks = v8::String::new(scope, &key).unwrap();
        pair.set_index(scope, 0, ks.into());
        pair.set_index(scope, 1, val);
        pairs.set_index(scope, pairs.length(), pair.into());
    }
}

fn usp_has(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let key = args.get(0).to_rust_string_lossy(scope);
    let Some(pairs) = usp_get_pairs(scope, args.this()) else { return };
    for i in 0..pairs.length() {
        if let Some(pair) = pairs.get_index(scope, i) {
            let pair = unsafe { v8::Local::<v8::Array>::cast_unchecked(pair) };
            if let Some(k) = pair.get_index(scope, 0) {
                if k.to_rust_string_lossy(scope) == key {
                    rv.set(v8::Boolean::new(scope, true).into());
                    return;
                }
            }
        }
    }
    rv.set(v8::Boolean::new(scope, false).into());
}

fn usp_delete(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue) {
    let key = args.get(0).to_rust_string_lossy(scope);
    let Some(pairs) = usp_get_pairs(scope, args.this()) else { return };
    // Rebuild array without matching keys
    let new_pairs = v8::Array::new(scope, 0);
    let mut idx = 0u32;
    for i in 0..pairs.length() {
        if let Some(pair) = pairs.get_index(scope, i) {
            let pair_arr = unsafe { v8::Local::<v8::Array>::cast_unchecked(pair) };
            if let Some(k) = pair_arr.get_index(scope, 0) {
                if k.to_rust_string_lossy(scope) != key {
                    new_pairs.set_index(scope, idx, pair);
                    idx += 1;
                }
            }
        }
    }
    let pk_name = v8::String::new(scope, "__pairs").unwrap();
    let hidden_key = v8::Private::for_api(scope, Some(pk_name));
    args.this().set_private(scope, hidden_key, new_pairs.into());
}

fn usp_append(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue) {
    let key = args.get(0);
    let val = args.get(1);
    let Some(pairs) = usp_get_pairs(scope, args.this()) else { return };
    let pair = v8::Array::new(scope, 2);
    pair.set_index(scope, 0, key);
    pair.set_index(scope, 1, val);
    pairs.set_index(scope, pairs.length(), pair.into());
}

fn usp_to_string(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let Some(pairs) = usp_get_pairs(scope, args.this()) else { return };
    let mut parts = Vec::new();
    for i in 0..pairs.length() {
        if let Some(pair) = pairs.get_index(scope, i) {
            let pair = unsafe { v8::Local::<v8::Array>::cast_unchecked(pair) };
            let k = pair.get_index(scope, 0).map(|v| v.to_rust_string_lossy(scope)).unwrap_or_default();
            let v = pair.get_index(scope, 1).map(|v| v.to_rust_string_lossy(scope)).unwrap_or_default();
            parts.push(format!("{}={}", url_encode(&k), url_encode(&v)));
        }
    }
    let result = parts.join("&");
    let v = v8::String::new(scope, &result).unwrap();
    rv.set(v.into());
}

fn usp_for_each(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue) {
    let callback = args.get(0);
    if !callback.is_function() { return; }
    let func = unsafe { v8::Local::<v8::Function>::cast_unchecked(callback) };
    let Some(pairs) = usp_get_pairs(scope, args.this()) else { return };
    let undefined = v8::undefined(scope);
    for i in 0..pairs.length() {
        if let Some(pair) = pairs.get_index(scope, i) {
            let pair = unsafe { v8::Local::<v8::Array>::cast_unchecked(pair) };
            let k = pair.get_index(scope, 0).unwrap_or_else(|| v8::undefined(scope).into());
            let v = pair.get_index(scope, 1).unwrap_or_else(|| v8::undefined(scope).into());
            func.call(scope, undefined.into(), &[v, k, args.this().into()]);
        }
    }
}

fn usp_entries(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let Some(pairs) = usp_get_pairs(scope, args.this()) else { return };
    let arr = v8::Array::new(scope, pairs.length() as i32);
    for i in 0..pairs.length() {
        if let Some(pair) = pairs.get_index(scope, i) {
            arr.set_index(scope, i, pair);
        }
    }
    rv.set(arr.into());
}

fn usp_keys(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let Some(pairs) = usp_get_pairs(scope, args.this()) else { return };
    let arr = v8::Array::new(scope, pairs.length() as i32);
    for i in 0..pairs.length() {
        if let Some(pair) = pairs.get_index(scope, i) {
            let pair = unsafe { v8::Local::<v8::Array>::cast_unchecked(pair) };
            if let Some(k) = pair.get_index(scope, 0) {
                arr.set_index(scope, i, k);
            }
        }
    }
    rv.set(arr.into());
}

fn usp_values(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let Some(pairs) = usp_get_pairs(scope, args.this()) else { return };
    let arr = v8::Array::new(scope, pairs.length() as i32);
    for i in 0..pairs.length() {
        if let Some(pair) = pairs.get_index(scope, i) {
            let pair = unsafe { v8::Local::<v8::Array>::cast_unchecked(pair) };
            if let Some(v) = pair.get_index(scope, 1) {
                arr.set_index(scope, i, v);
            }
        }
    }
    rv.set(arr.into());
}
