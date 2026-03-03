/// XMLHttpRequest bindings using the unified fetch pipeline.
///
/// Provides the XMLHttpRequest constructor and all instance methods.
/// The send() method uses Request::xhr() + unified fetch pipeline instead
/// of inline reqwest calls.

/// Install XMLHttpRequest on the global object.
pub fn install(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    let xhr_ctor = v8::Function::new(scope, xhr_constructor).unwrap();
    for (name, ival) in &[("UNSENT", 0), ("OPENED", 1), ("HEADERS_RECEIVED", 2), ("LOADING", 3), ("DONE", 4)] {
        let k = v8::String::new(scope, name).unwrap();
        let v = v8::Integer::new(scope, *ival);
        xhr_ctor.set(scope, k.into(), v.into());
    }
    let key = v8::String::new(scope, "XMLHttpRequest").unwrap();
    global.set(scope, key.into(), xhr_ctor.into());
}

fn xhr_constructor(scope: &mut v8::HandleScope, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let obj = v8::Object::new(scope);
    let empty = v8::String::new(scope, "").unwrap();
    let izero = v8::Integer::new(scope, 0);
    let null = v8::null(scope);

    let k = v8::String::new(scope, "readyState").unwrap();
    obj.set(scope, k.into(), izero.into());
    let k = v8::String::new(scope, "status").unwrap();
    obj.set(scope, k.into(), izero.into());
    let k = v8::String::new(scope, "statusText").unwrap();
    obj.set(scope, k.into(), empty.into());
    let k = v8::String::new(scope, "responseText").unwrap();
    obj.set(scope, k.into(), empty.into());
    let k = v8::String::new(scope, "response").unwrap();
    obj.set(scope, k.into(), empty.into());
    let k = v8::String::new(scope, "responseType").unwrap();
    obj.set(scope, k.into(), empty.into());
    let k = v8::String::new(scope, "responseURL").unwrap();
    obj.set(scope, k.into(), empty.into());
    let k = v8::String::new(scope, "timeout").unwrap();
    obj.set(scope, k.into(), izero.into());
    let k = v8::String::new(scope, "withCredentials").unwrap();
    let val = v8::Boolean::new(scope, false);
    obj.set(scope, k.into(), val.into());

    for name in &["onload", "onerror", "onreadystatechange", "onprogress", "onloadstart", "onloadend", "ontimeout", "onabort"] {
        let k = v8::String::new(scope, name).unwrap();
        obj.set(scope, k.into(), null.into());
    }

    // Private storage
    let pk = v8::String::new(scope, "__xhrMethod").unwrap();
    let hk = v8::Private::for_api(scope, Some(pk));
    let v = v8::String::new(scope, "GET").unwrap();
    obj.set_private(scope, hk, v.into());
    let pk = v8::String::new(scope, "__xhrUrl").unwrap();
    let hk = v8::Private::for_api(scope, Some(pk));
    obj.set_private(scope, hk, empty.into());
    let pk = v8::String::new(scope, "__xhrHeaders").unwrap();
    let hk = v8::Private::for_api(scope, Some(pk));
    let val = v8::Array::new(scope, 0);
    obj.set_private(scope, hk, val.into());
    let pk = v8::String::new(scope, "__xhrRespHeaders").unwrap();
    let hk = v8::Private::for_api(scope, Some(pk));
    obj.set_private(scope, hk, empty.into());

    for (name, ival) in &[("UNSENT", 0), ("OPENED", 1), ("HEADERS_RECEIVED", 2), ("LOADING", 3), ("DONE", 4)] {
        let k = v8::String::new(scope, name).unwrap();
        let v = v8::Integer::new(scope, *ival);
        obj.set(scope, k.into(), v.into());
    }

    let open_fn = v8::Function::new(scope, xhr_open).unwrap();
    let k = v8::String::new(scope, "open").unwrap();
    obj.set(scope, k.into(), open_fn.into());
    let send_fn = v8::Function::new(scope, xhr_send).unwrap();
    let k = v8::String::new(scope, "send").unwrap();
    obj.set(scope, k.into(), send_fn.into());
    let set_header_fn = v8::Function::new(scope, xhr_set_request_header).unwrap();
    let k = v8::String::new(scope, "setRequestHeader").unwrap();
    obj.set(scope, k.into(), set_header_fn.into());
    let get_header_fn = v8::Function::new(scope, xhr_get_response_header).unwrap();
    let k = v8::String::new(scope, "getResponseHeader").unwrap();
    obj.set(scope, k.into(), get_header_fn.into());
    let get_all_fn = v8::Function::new(scope, xhr_get_all_response_headers).unwrap();
    let k = v8::String::new(scope, "getAllResponseHeaders").unwrap();
    obj.set(scope, k.into(), get_all_fn.into());

    let noop = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    for name in &["abort", "overrideMimeType", "addEventListener", "removeEventListener"] {
        let k = v8::String::new(scope, name).unwrap();
        obj.set(scope, k.into(), noop.into());
    }

    rv.set(obj.into());
}

fn xhr_open(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue) {
    let this = args.this();
    let method = args.get(0).to_rust_string_lossy(scope).to_ascii_uppercase();
    let url = args.get(1).to_rust_string_lossy(scope);
    let pk = v8::String::new(scope, "__xhrMethod").unwrap();
    let hk = v8::Private::for_api(scope, Some(pk));
    let v = v8::String::new(scope, &method).unwrap();
    this.set_private(scope, hk, v.into());
    let pk = v8::String::new(scope, "__xhrUrl").unwrap();
    let hk = v8::Private::for_api(scope, Some(pk));
    let v = v8::String::new(scope, &url).unwrap();
    this.set_private(scope, hk, v.into());
    let k = v8::String::new(scope, "readyState").unwrap();
    let val = v8::Integer::new(scope, 1);
    this.set(scope, k.into(), val.into());
}

fn xhr_set_request_header(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue) {
    let this = args.this();
    let name = args.get(0).to_rust_string_lossy(scope);
    let value = args.get(1).to_rust_string_lossy(scope);
    let pk = v8::String::new(scope, "__xhrHeaders").unwrap();
    let hk = v8::Private::for_api(scope, Some(pk));
    if let Some(arr_val) = this.get_private(scope, hk) {
        if let Ok(arr) = v8::Local::<v8::Array>::try_from(arr_val) {
            let pair = v8::Array::new(scope, 2);
            let n = v8::String::new(scope, &name).unwrap();
            let v = v8::String::new(scope, &value).unwrap();
            pair.set_index(scope, 0, n.into());
            pair.set_index(scope, 1, v.into());
            arr.set_index(scope, arr.length(), pair.into());
        }
    }
}

fn xhr_send(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue) {
    let this = args.this();
    let pk = v8::String::new(scope, "__xhrMethod").unwrap();
    let hk = v8::Private::for_api(scope, Some(pk));
    let method = this.get_private(scope, hk).map(|v| v.to_rust_string_lossy(scope)).unwrap_or_else(|| "GET".to_string());

    let pk = v8::String::new(scope, "__xhrUrl").unwrap();
    let hk = v8::Private::for_api(scope, Some(pk));
    let url_str = this.get_private(scope, hk).map(|v| v.to_rust_string_lossy(scope)).unwrap_or_default();
    if url_str.is_empty() { return; }

    let base_url = scope.get_slot::<super::location::BaseUrl>().and_then(|b| b.0.clone());
    let resolved = match crate::net::fetch::resolve_url(&url_str, base_url.as_deref()) {
        Ok(u) => u,
        Err(e) => {
            log::warn!("[xhr] URL resolve failed for '{}': {}", url_str, e);
            return;
        }
    };

    // Extract request headers from V8 private storage
    let pk = v8::String::new(scope, "__xhrHeaders").unwrap();
    let hk = v8::Private::for_api(scope, Some(pk));
    let mut req_headers: Vec<(String, String)> = Vec::new();
    if let Some(arr_val) = this.get_private(scope, hk) {
        if let Ok(arr) = v8::Local::<v8::Array>::try_from(arr_val) {
            for i in 0..arr.length() {
                if let Some(pair_val) = arr.get_index(scope, i) {
                    if let Ok(pair) = v8::Local::<v8::Array>::try_from(pair_val) {
                        let n = pair.get_index(scope, 0).map(|v| v.to_rust_string_lossy(scope)).unwrap_or_default();
                        let v = pair.get_index(scope, 1).map(|v| v.to_rust_string_lossy(scope)).unwrap_or_default();
                        req_headers.push((n, v));
                    }
                }
            }
        }
    }

    let body_arg = args.get(0);
    let body = if body_arg.is_null() || body_arg.is_undefined() { None } else { Some(body_arg.to_rust_string_lossy(scope)) };

    // Build a unified Request
    let http_method: reqwest::Method = method.parse().unwrap_or(reqwest::Method::GET);
    let mut request = crate::net::request::Request::xhr(resolved.clone(), http_method);

    // Apply headers
    for (n, v) in &req_headers {
        if let Ok(val) = reqwest::header::HeaderValue::from_str(v) {
            if let Ok(name) = n.parse::<reqwest::header::HeaderName>() {
                request.headers.insert(name, val);
            }
        }
    }

    // Apply body
    if let Some(b) = &body {
        request.body = Some(b.as_bytes().to_vec());
    }

    log::debug!("[xhr] {} {} ({} headers)", request.method, resolved, req_headers.len());

    // Execute through unified pipeline (blocking — XHR is synchronous)
    // Use shared FetchContext from isolate slot if available (shares cache/cookies)
    let context = scope
        .get_slot::<crate::net::fetch::FetchContext>()
        .cloned()
        .unwrap_or_else(|| crate::net::fetch::FetchContext::new(base_url.as_deref()));
    let response = crate::net::fetch::fetch(&mut request, &context);

    log::debug!(
        "[xhr] {} {} → {} ({} bytes)",
        method, resolved, response.status, response.body.len(),
    );

    // Helper: fire onreadystatechange callback if set
    let fire_readystatechange = |scope: &mut v8::HandleScope, this: v8::Local<v8::Object>| {
        let k = v8::String::new(scope, "onreadystatechange").unwrap();
        if let Some(cb) = this.get(scope, k.into()) {
            if cb.is_function() {
                let func = unsafe { v8::Local::<v8::Function>::cast_unchecked(cb) };
                let undef = v8::undefined(scope);
                func.call(scope, undef.into(), &[]);
            }
        }
    };

    if response.is_network_error() {
        log::warn!("[xhr] {} {} network error: {}", method, resolved, response.status_text);
        let k = v8::String::new(scope, "readyState").unwrap();
        let val = v8::Integer::new(scope, 4);
        this.set(scope, k.into(), val.into());
        let k = v8::String::new(scope, "onerror").unwrap();
        if let Some(cb) = this.get(scope, k.into()) {
            if cb.is_function() {
                let func = unsafe { v8::Local::<v8::Function>::cast_unchecked(cb) };
                let undef = v8::undefined(scope);
                func.call(scope, undef.into(), &[]);
            }
        }
        return;
    }

    // Success path — set status, headers, body per XHR spec
    let status = response.status;
    let status_text = &response.status_text;
    let response_text = response.text();
    let final_url = response.final_url()
        .map(|u| u.as_str().to_owned())
        .unwrap_or_else(|| resolved.as_str().to_owned());

    // Build response headers string
    let headers_str: String = response.headers.iter()
        .map(|(k, v)| format!("{}: {}", k, v.to_str().unwrap_or("")))
        .collect::<Vec<_>>()
        .join("\r\n");

    // readyState 2 (HEADERS_RECEIVED)
    {
        let k = v8::String::new(scope, "readyState").unwrap();
        let v = v8::Integer::new(scope, 2);
        this.set(scope, k.into(), v.into());
    }
    let k = v8::String::new(scope, "status").unwrap();
    let val = v8::Integer::new(scope, status as i32);
    this.set(scope, k.into(), val.into());
    let k = v8::String::new(scope, "statusText").unwrap();
    let v = v8::String::new(scope, status_text).unwrap();
    this.set(scope, k.into(), v.into());
    let pk = v8::String::new(scope, "__xhrRespHeaders").unwrap();
    let hk = v8::Private::for_api(scope, Some(pk));
    let v = v8::String::new(scope, &headers_str).unwrap();
    this.set_private(scope, hk, v.into());
    let k = v8::String::new(scope, "responseURL").unwrap();
    let v = v8::String::new(scope, &final_url).unwrap();
    this.set(scope, k.into(), v.into());
    fire_readystatechange(scope, this);

    // readyState 3 (LOADING)
    {
        let k = v8::String::new(scope, "readyState").unwrap();
        let v = v8::Integer::new(scope, 3);
        this.set(scope, k.into(), v.into());
    }
    fire_readystatechange(scope, this);

    // readyState 4 (DONE)
    {
        let k = v8::String::new(scope, "readyState").unwrap();
        let v = v8::Integer::new(scope, 4);
        this.set(scope, k.into(), v.into());
    }
    let k = v8::String::new(scope, "responseText").unwrap();
    let v = v8::String::new(scope, &response_text).unwrap();
    this.set(scope, k.into(), v.into());
    let k = v8::String::new(scope, "response").unwrap();
    this.set(scope, k.into(), v.into());
    fire_readystatechange(scope, this);

    // Fire onload and onloadend
    for cb_name in &["onload", "onloadend"] {
        let k = v8::String::new(scope, cb_name).unwrap();
        if let Some(cb) = this.get(scope, k.into()) {
            if cb.is_function() {
                let func = unsafe { v8::Local::<v8::Function>::cast_unchecked(cb) };
                let undef = v8::undefined(scope);
                func.call(scope, undef.into(), &[]);
            }
        }
    }
}

fn xhr_get_response_header(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let this = args.this();
    let name = args.get(0).to_rust_string_lossy(scope).to_ascii_lowercase();
    let pk = v8::String::new(scope, "__xhrRespHeaders").unwrap();
    let hk = v8::Private::for_api(scope, Some(pk));
    if let Some(headers_val) = this.get_private(scope, hk) {
        let headers_str = headers_val.to_rust_string_lossy(scope);
        for line in headers_str.split("\r\n") {
            if let Some((k, v)) = line.split_once(": ") {
                if k.to_ascii_lowercase() == name {
                    rv.set(v8::String::new(scope, v).unwrap().into());
                    return;
                }
            }
        }
    }
    rv.set(v8::null(scope).into());
}

fn xhr_get_all_response_headers(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let this = args.this();
    let pk = v8::String::new(scope, "__xhrRespHeaders").unwrap();
    let hk = v8::Private::for_api(scope, Some(pk));
    if let Some(headers_val) = this.get_private(scope, hk) {
        rv.set(headers_val);
    } else {
        rv.set(v8::String::new(scope, "").unwrap().into());
    }
}
