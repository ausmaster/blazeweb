//! High-performance `fetch()` API implementation using the unified fetch pipeline.
//!
//! Architecture: scripts call `fetch(url, options?)` which enqueues a
//! `PendingFetch` and returns a Promise. After scripts complete, `drain()`
//! fires ALL pending fetches concurrently via the unified pipeline, resolves
//! the promises, then runs `perform_microtask_checkpoint()` so `.then()`
//! chains execute. If those handlers enqueue more fetches, drain repeats.

use wreq::header::HeaderValue;
use wreq::Method;

use crate::js::bindings::location::BaseUrl;
use crate::net::fetch::{resolve_url, FetchContext};
use crate::net::request::Request;
use crate::net::response::Response;

// ── Types ────────────────────────────────────────────────────────────────────

/// Queue of pending fetch requests, stored as an isolate slot.
pub struct FetchQueue {
    pub pending: Vec<PendingFetch>,
}

pub struct PendingFetch {
    url: String,
    method: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
    resolver: v8::Global<v8::PromiseResolver>,
}

impl FetchQueue {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }
}

// ── Install ──────────────────────────────────────────────────────────────────

/// Install `fetch` on the global object.
pub fn install(scope: &mut v8::PinnedRef<v8::HandleScope>) {
    let context = scope.get_current_context();
    let global = context.global(scope);
    let key = v8::String::new(scope, "fetch").unwrap();
    let func = v8::Function::new(scope, fetch_callback).unwrap();
    global.set(scope, key.into(), func.into());
}

// ── V8 callback ──────────────────────────────────────────────────────────────

fn fetch_callback(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    // Parse URL from first argument (string or object with .url)
    let url_str = {
        let arg0 = args.get(0);
        if arg0.is_string() {
            arg0.to_rust_string_lossy(scope)
        } else if arg0.is_object() {
            let obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(arg0) };
            let url_key = v8::String::new(scope, "url").unwrap();
            obj.get(scope, url_key.into())
                .map(|v| v.to_rust_string_lossy(scope))
                .unwrap_or_default()
        } else {
            String::new()
        }
    };

    // Parse options from second argument
    let (method, headers, body) = if args.length() > 1 && args.get(1).is_object() {
        let opts = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        let method = {
            let k = v8::String::new(scope, "method").unwrap();
            opts.get(scope, k.into())
                .filter(|v| !v.is_undefined())
                .map(|v| v.to_rust_string_lossy(scope).to_uppercase())
                .unwrap_or_else(|| "GET".into())
        };
        let headers = parse_headers(scope, opts);
        let body = {
            let k = v8::String::new(scope, "body").unwrap();
            opts.get(scope, k.into())
                .filter(|v| !v.is_undefined() && !v.is_null())
                .map(|v| v.to_rust_string_lossy(scope))
        };
        (method, headers, body)
    } else {
        ("GET".into(), vec![], None)
    };

    // Create promise
    let Some(resolver) = v8::PromiseResolver::new(scope) else {
        return;
    };
    let promise = resolver.get_promise(scope);
    rv.set(promise.into());

    // Enqueue
    let global_resolver = v8::Global::new(scope, resolver);
    let queue = scope.get_slot_mut::<FetchQueue>().unwrap();
    queue.pending.push(PendingFetch {
        url: url_str,
        method,
        headers,
        body,
        resolver: global_resolver,
    });
}

fn parse_headers(scope: &mut v8::PinnedRef<v8::HandleScope>, opts: v8::Local<v8::Object>) -> Vec<(String, String)> {
    let k = v8::String::new(scope, "headers").unwrap();
    let Some(val) = opts.get(scope, k.into()) else {
        return vec![];
    };
    if val.is_undefined() || val.is_null() {
        return vec![];
    }
    if !val.is_object() {
        return vec![];
    }
    let headers_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(val) };
    let Some(names) = headers_obj.get_own_property_names(scope, Default::default()) else {
        return vec![];
    };
    let mut result = Vec::new();
    for i in 0..names.length() {
        let key = names.get_index(scope, i).unwrap();
        let key_str = key.to_rust_string_lossy(scope);
        if let Some(val) = headers_obj.get(scope, key) {
            result.push((key_str, val.to_rust_string_lossy(scope)));
        }
    }
    result
}

// ── Drain (batch execute + resolve) ──────────────────────────────────────────

/// Drain all pending fetches: fire concurrently via unified pipeline, resolve
/// promises, run microtasks. Returns collected error messages.
pub fn drain(scope: &mut v8::PinnedRef<v8::HandleScope>, max_rounds: usize) -> Vec<String> {
    let errors = Vec::new();

    for _ in 0..max_rounds {
        // Take all pending fetches
        let queue = scope.get_slot_mut::<FetchQueue>().unwrap();
        if queue.pending.is_empty() {
            break;
        }
        let pending = std::mem::take(&mut queue.pending);
        let count = pending.len();
        log::debug!("[js:fetch] draining {} pending fetch(es)", count);

        // Resolve URLs and build Request objects
        let base_url = scope
            .get_slot::<BaseUrl>()
            .and_then(|b| b.0.as_deref())
            .map(|s| s.to_string());
        let base_ref = base_url.as_deref();

        let mut requests: Vec<(usize, Request)> = Vec::new();
        let mut resolvers: Vec<v8::Global<v8::PromiseResolver>> = Vec::new();

        for (i, pf) in pending.into_iter().enumerate() {
            resolvers.push(pf.resolver);
            match resolve_url(&pf.url, base_ref) {
                Ok(url) => {
                    let method: Method = pf.method.parse().unwrap_or(Method::GET);
                    let mut request = Request::fetch_api(url, method);
                    // Apply user-specified headers
                    for (k, v) in &pf.headers {
                        if let Ok(val) = HeaderValue::from_str(v) {
                            if let Ok(name) = k.parse::<wreq::header::HeaderName>() {
                                request.headers.insert(name, val);
                            }
                        }
                    }
                    // Apply body
                    if let Some(body) = pf.body {
                        request.body = Some(body.into_bytes());
                    }
                    requests.push((i, request));
                }
                Err(e) => {
                    log::warn!("[js:fetch] URL resolve failed for '{}': {}", pf.url, e);
                    let resolver = v8::Local::new(scope, &resolvers[i]);
                    let msg = v8::String::new(scope, &format!("Failed to fetch: {}", e)).unwrap();
                    let err = v8::Exception::type_error(scope, msg);
                    resolver.reject(scope, err);
                }
            }
        }

        if requests.is_empty() {
            scope.perform_microtask_checkpoint();
            continue;
        }

        // Fire ALL requests concurrently via unified pipeline (using shared context)
        let context = scope
            .get_slot::<FetchContext>()
            .cloned()
            .unwrap_or_else(|| FetchContext::new(base_ref));
        let results = crate::net::fetch::fetch_parallel(requests, &context);

        // Resolve/reject each promise
        for (idx, response) in results {
            let resolver = v8::Local::new(scope, &resolvers[idx]);
            if response.is_network_error() {
                let err_msg = v8::String::new(
                    scope,
                    &format!("Failed to fetch: {}", response.status_text),
                ).unwrap();
                let err = v8::Exception::type_error(scope, err_msg);
                resolver.reject(scope, err);
            } else {
                let response_obj = create_response_object(scope, &response);
                resolver.resolve(scope, response_obj.into());
            }
        }

        // Run microtask checkpoint — executes .then() chains
        scope.perform_microtask_checkpoint();
        log::debug!("[js:fetch] drain round complete ({} requests)", count);
    }

    errors
}

// ── Async helpers for parallel fetch within drain ────────────────────────────

// (fetch_parallel in net/fetch.rs handles the JoinSet concurrency)

// ── Response object construction ─────────────────────────────────────────────

fn create_response_object<'s, 'i>(
    scope: &mut v8::PinnedRef<'s, v8::HandleScope<'i>>,
    resp: &Response,
) -> v8::Local<'s, v8::Object> {
    let obj = v8::Object::new(scope);

    // status (number)
    let k = v8::String::new(scope, "status").unwrap();
    let v = v8::Integer::new(scope, resp.status as i32);
    obj.set(scope, k.into(), v.into());

    // statusText
    let k = v8::String::new(scope, "statusText").unwrap();
    let v = v8::String::new(scope, &resp.status_text).unwrap();
    obj.set(scope, k.into(), v.into());

    // ok (status 200-299)
    let k = v8::String::new(scope, "ok").unwrap();
    let v = v8::Boolean::new(scope, resp.ok());
    obj.set(scope, k.into(), v.into());

    // url
    let k = v8::String::new(scope, "url").unwrap();
    let url_str = resp.final_url().map(|u| u.as_str()).unwrap_or("");
    let v = v8::String::new(scope, url_str).unwrap();
    obj.set(scope, k.into(), v.into());

    // type
    let k = v8::String::new(scope, "type").unwrap();
    let v = v8::String::new(scope, "basic").unwrap();
    obj.set(scope, k.into(), v.into());

    // redirected
    let k = v8::String::new(scope, "redirected").unwrap();
    let v = v8::Boolean::new(scope, resp.was_redirected());
    obj.set(scope, k.into(), v.into());

    // bodyUsed
    let k = v8::String::new(scope, "bodyUsed").unwrap();
    let v = v8::Boolean::new(scope, false);
    obj.set(scope, k.into(), v.into());

    // Store body text in a private key for text()/json()
    let body_text = resp.text();
    let body_name = v8::String::new(scope, "__body").unwrap();
    let body_key = v8::Private::for_api(scope, Some(body_name));
    let body_val = v8::String::new(scope, &body_text).unwrap();
    obj.set_private(scope, body_key, body_val.into());

    // body — ReadableStream wrapping the response body bytes
    let body_bytes = body_text.as_bytes();
    let body_stream = crate::js::bindings::streams::create_from_bytes(scope, body_bytes);
    let k = v8::String::new(scope, "body").unwrap();
    obj.set(scope, k.into(), body_stream.into());

    // headers object
    let resp_headers: Vec<(String, String)> = resp.headers.iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let headers_obj = create_headers_object(scope, &resp_headers);
    let k = v8::String::new(scope, "headers").unwrap();
    obj.set(scope, k.into(), headers_obj.into());

    // text() method
    let text_fn = v8::Function::new(scope, response_text).unwrap();
    let k = v8::String::new(scope, "text").unwrap();
    obj.set(scope, k.into(), text_fn.into());

    // json() method
    let json_fn = v8::Function::new(scope, response_json).unwrap();
    let k = v8::String::new(scope, "json").unwrap();
    obj.set(scope, k.into(), json_fn.into());

    // arrayBuffer() stub — returns empty resolved promise
    let ab_fn = v8::Function::new(scope, response_array_buffer).unwrap();
    let k = v8::String::new(scope, "arrayBuffer").unwrap();
    obj.set(scope, k.into(), ab_fn.into());

    // blob() stub
    let blob_fn = v8::Function::new(scope, response_blob).unwrap();
    let k = v8::String::new(scope, "blob").unwrap();
    obj.set(scope, k.into(), blob_fn.into());

    // clone() method
    let clone_fn = v8::Function::new(scope, response_clone).unwrap();
    let k = v8::String::new(scope, "clone").unwrap();
    obj.set(scope, k.into(), clone_fn.into());

    obj
}

fn get_body_from_response(scope: &mut v8::PinnedRef<v8::HandleScope>, this: v8::Local<v8::Object>) -> String {
    let body_name = v8::String::new(scope, "__body").unwrap();
    let body_key = v8::Private::for_api(scope, Some(body_name));
    this.get_private(scope, body_key)
        .map(|v| v.to_rust_string_lossy(scope))
        .unwrap_or_default()
}

fn response_text(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    let body = get_body_from_response(scope, this);

    let k = v8::String::new(scope, "bodyUsed").unwrap();
    let v = v8::Boolean::new(scope, true);
    this.set(scope, k.into(), v.into());

    let Some(resolver) = v8::PromiseResolver::new(scope) else { return };
    let body_val = v8::String::new(scope, &body).unwrap();
    resolver.resolve(scope, body_val.into());
    rv.set(resolver.get_promise(scope).into());
}

fn response_json(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    let body = get_body_from_response(scope, this);

    let k = v8::String::new(scope, "bodyUsed").unwrap();
    let v = v8::Boolean::new(scope, true);
    this.set(scope, k.into(), v.into());

    let Some(resolver) = v8::PromiseResolver::new(scope) else { return };

    let body_val = v8::String::new(scope, &body).unwrap();
    match v8::json::parse(scope, body_val.into()) {
        Some(parsed) => {
            resolver.resolve(scope, parsed);
        }
        None => {
            let msg = v8::String::new(scope, "Unexpected end of JSON input").unwrap();
            let err = v8::Exception::syntax_error(scope, msg);
            resolver.reject(scope, err);
        }
    }
    rv.set(resolver.get_promise(scope).into());
}

fn response_array_buffer(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(resolver) = v8::PromiseResolver::new(scope) else { return };
    let ab = v8::ArrayBuffer::new(scope, 0);
    resolver.resolve(scope, ab.into());
    rv.set(resolver.get_promise(scope).into());
}

fn response_blob(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(resolver) = v8::PromiseResolver::new(scope) else { return };
    let obj = v8::Object::new(scope);
    resolver.resolve(scope, obj.into());
    rv.set(resolver.get_promise(scope).into());
}

fn response_clone(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    let body = get_body_from_response(scope, this);

    let k = v8::String::new(scope, "status").unwrap();
    let status = this.get(scope, k.into())
        .and_then(|v| v.int32_value(scope))
        .unwrap_or(0) as u16;

    let k = v8::String::new(scope, "statusText").unwrap();
    let status_text = this.get(scope, k.into())
        .map(|v| v.to_rust_string_lossy(scope))
        .unwrap_or_default();

    let k = v8::String::new(scope, "url").unwrap();
    let url = this.get(scope, k.into())
        .map(|v| v.to_rust_string_lossy(scope))
        .unwrap_or_default();

    // Build a Response for cloning
    let url_list = if let Ok(parsed) = url::Url::parse(&url) {
        vec![parsed]
    } else {
        vec![]
    };
    let resp = Response {
        response_type: crate::net::response::ResponseType::Basic,
        status,
        status_text,
        headers: wreq::header::HeaderMap::new(),
        body: body.into_bytes(),
        url_list,
    };
    let clone = create_response_object(scope, &resp);
    rv.set(clone.into());
}

// ── Headers object ───────────────────────────────────────────────────────────

fn create_headers_object<'s, 'i>(
    scope: &mut v8::PinnedRef<'s, v8::HandleScope<'i>>,
    headers: &[(String, String)],
) -> v8::Local<'s, v8::Object> {
    let obj = v8::Object::new(scope);

    // Store headers as a serialized array in a private key
    let mut serialized = String::new();
    for (k, v) in headers {
        if !serialized.is_empty() {
            serialized.push('\n');
        }
        serialized.push_str(&k.to_lowercase());
        serialized.push('\0');
        serialized.push_str(v);
    }
    let name = v8::String::new(scope, "__headers").unwrap();
    let key = v8::Private::for_api(scope, Some(name));
    let val = v8::String::new(scope, &serialized).unwrap();
    obj.set_private(scope, key, val.into());

    let get_fn = v8::Function::new(scope, headers_get).unwrap();
    let k = v8::String::new(scope, "get").unwrap();
    obj.set(scope, k.into(), get_fn.into());

    let has_fn = v8::Function::new(scope, headers_has).unwrap();
    let k = v8::String::new(scope, "has").unwrap();
    obj.set(scope, k.into(), has_fn.into());

    let foreach_fn = v8::Function::new(scope, headers_foreach).unwrap();
    let k = v8::String::new(scope, "forEach").unwrap();
    obj.set(scope, k.into(), foreach_fn.into());

    obj
}

fn get_headers_map(scope: &mut v8::PinnedRef<v8::HandleScope>, this: v8::Local<v8::Object>) -> Vec<(String, String)> {
    let name = v8::String::new(scope, "__headers").unwrap();
    let key = v8::Private::for_api(scope, Some(name));
    let serialized = this
        .get_private(scope, key)
        .map(|v| v.to_rust_string_lossy(scope))
        .unwrap_or_default();

    if serialized.is_empty() {
        return vec![];
    }

    serialized
        .split('\n')
        .filter_map(|entry| {
            let mut parts = entry.splitn(2, '\0');
            let k = parts.next()?;
            let v = parts.next()?;
            Some((k.to_string(), v.to_string()))
        })
        .collect()
}

fn headers_get(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let name = args.get(0).to_rust_string_lossy(scope).to_lowercase();
    let headers = get_headers_map(scope, args.this());

    let value = headers
        .iter()
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.as_str());

    match value {
        Some(v) => {
            let val = v8::String::new(scope, v).unwrap();
            rv.set(val.into());
        }
        None => {
            rv.set(v8::null(scope).into());
        }
    }
}

fn headers_has(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let name = args.get(0).to_rust_string_lossy(scope).to_lowercase();
    let headers = get_headers_map(scope, args.this());
    let found = headers.iter().any(|(k, _)| *k == name);
    rv.set(v8::Boolean::new(scope, found).into());
}

fn headers_foreach(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let callback = args.get(0);
    if !callback.is_function() {
        return;
    }
    let func = unsafe { v8::Local::<v8::Function>::cast_unchecked(callback) };
    let headers = get_headers_map(scope, args.this());
    let this = args.this();

    for (k, v) in &headers {
        let key_val = v8::String::new(scope, k).unwrap();
        let val_val = v8::String::new(scope, v).unwrap();
        let args_arr: [v8::Local<v8::Value>; 3] = [val_val.into(), key_val.into(), this.into()];
        func.call(scope, this.into(), &args_arr);
    }
}
