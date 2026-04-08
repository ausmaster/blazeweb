/// window.navigator — minimal stub for SSR compatibility.

pub fn create_navigator_object<'s>(
    scope: &mut v8::HandleScope<'s>,
) -> v8::Local<'s, v8::Object> {
    let obj = v8::Object::new(scope);

    set_str(scope, obj, "userAgent",
        "Mozilla/5.0 (compatible; blazeweb/0.1; +https://github.com/AustinScola/blazeweb)");
    set_str(scope, obj, "appName", "Netscape");
    set_str(scope, obj, "appVersion", "5.0");
    set_str(scope, obj, "platform", "Linux x86_64");
    set_str(scope, obj, "language", "en");
    set_str(scope, obj, "vendor", "");
    set_str(scope, obj, "product", "Gecko");

    // languages array
    let langs = v8::Array::new(scope, 1);
    let en = v8::String::new(scope, "en").unwrap();
    langs.set_index(scope, 0, en.into());
    let k = v8::String::new(scope, "languages").unwrap();
    obj.set(scope, k.into(), langs.into());

    // Booleans
    set_bool(scope, obj, "onLine", true);
    set_bool(scope, obj, "cookieEnabled", true);
    set_bool(scope, obj, "javaEnabled", false);

    // Numbers
    let k = v8::String::new(scope, "hardwareConcurrency").unwrap();
    let v = v8::Integer::new(scope, 4);
    obj.set(scope, k.into(), v.into());

    // No-op methods
    let noop = v8::Function::new(scope, |_scope: &mut v8::HandleScope, _args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue| {}).unwrap();
    for name in &["sendBeacon", "vibrate"] {
        let k = v8::String::new(scope, name).unwrap();
        obj.set(scope, k.into(), noop.into());
    }

    // javaEnabled() method
    let java_enabled = v8::Function::new(scope, |scope: &mut v8::HandleScope, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        rv.set(v8::Boolean::new(scope, false).into());
    }).unwrap();
    let k = v8::String::new(scope, "javaEnabled").unwrap();
    obj.set(scope, k.into(), java_enabled.into());

    // serviceWorker — stub for PWA feature detection
    let sw = v8::Object::new(scope);

    // register() — returns resolved promise
    let register_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope,
        _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        log::trace!("navigator.serviceWorker.register() called (no-op in SSR)");
        let resolver = v8::PromiseResolver::new(scope).unwrap();
        let result = v8::Object::new(scope);
        let k = v8::String::new(scope, "scope").unwrap();
        let v = v8::String::new(scope, "/").unwrap();
        result.set(scope, k.into(), v.into());
        resolver.resolve(scope, result.into());
        rv.set(resolver.get_promise(scope).into());
    }).unwrap();
    let k = v8::String::new(scope, "register").unwrap();
    sw.set(scope, k.into(), register_fn.into());

    // ready — resolved promise
    let resolver = v8::PromiseResolver::new(scope).unwrap();
    let undef = v8::undefined(scope);
    resolver.resolve(scope, undef.into());
    let ready_promise = resolver.get_promise(scope);
    let k = v8::String::new(scope, "ready").unwrap();
    sw.set(scope, k.into(), ready_promise.into());

    // controller — null (no active SW)
    let k = v8::String::new(scope, "controller").unwrap();
    let null_val = v8::null(scope);
    sw.set(scope, k.into(), null_val.into());

    let k = v8::String::new(scope, "serviceWorker").unwrap();
    obj.set(scope, k.into(), sw.into());

    // ─── Round 2 Phase 4: Navigator stubs ───────────────────────────────

    // clipboard
    let clipboard = v8::Object::new(scope);
    let write_text_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let resolver = v8::PromiseResolver::new(scope).unwrap();
        let undef = v8::undefined(scope);
        resolver.resolve(scope, undef.into());
        rv.set(resolver.get_promise(scope).into());
    }).unwrap();
    for name in &["writeText", "write"] {
        let k = v8::String::new(scope, name).unwrap();
        clipboard.set(scope, k.into(), write_text_fn.into());
    }
    let read_text_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        // Reject — clipboard not available in SSR
        let resolver = v8::PromiseResolver::new(scope).unwrap();
        let msg = v8::String::new(scope, "NotAllowedError: Clipboard read not available in SSR").unwrap();
        let err = v8::Exception::error(scope, msg);
        resolver.reject(scope, err);
        rv.set(resolver.get_promise(scope).into());
    }).unwrap();
    for name in &["readText", "read"] {
        let k = v8::String::new(scope, name).unwrap();
        clipboard.set(scope, k.into(), read_text_fn.into());
    }
    let k = v8::String::new(scope, "clipboard").unwrap();
    obj.set(scope, k.into(), clipboard.into());

    // permissions
    let permissions = v8::Object::new(scope);
    let query_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let resolver = v8::PromiseResolver::new(scope).unwrap();
        let result = v8::Object::new(scope);
        let k = v8::String::new(scope, "state").unwrap();
        let v = v8::String::new(scope, "prompt").unwrap();
        result.set(scope, k.into(), v.into());
        let null = v8::null(scope);
        let k = v8::String::new(scope, "onchange").unwrap();
        result.set(scope, k.into(), null.into());
        resolver.resolve(scope, result.into());
        rv.set(resolver.get_promise(scope).into());
    }).unwrap();
    let k = v8::String::new(scope, "query").unwrap();
    permissions.set(scope, k.into(), query_fn.into());
    let k = v8::String::new(scope, "permissions").unwrap();
    obj.set(scope, k.into(), permissions.into());

    // mediaDevices
    let media_devices = v8::Object::new(scope);
    let enum_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let resolver = v8::PromiseResolver::new(scope).unwrap();
        let arr = v8::Array::new(scope, 0);
        resolver.resolve(scope, arr.into());
        rv.set(resolver.get_promise(scope).into());
    }).unwrap();
    let k = v8::String::new(scope, "enumerateDevices").unwrap();
    media_devices.set(scope, k.into(), enum_fn.into());
    let gum_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let resolver = v8::PromiseResolver::new(scope).unwrap();
        let msg = v8::String::new(scope, "NotAllowedError: getUserMedia not available in SSR").unwrap();
        let err = v8::Exception::error(scope, msg);
        resolver.reject(scope, err);
        rv.set(resolver.get_promise(scope).into());
    }).unwrap();
    let k = v8::String::new(scope, "getUserMedia").unwrap();
    media_devices.set(scope, k.into(), gum_fn.into());
    let k = v8::String::new(scope, "mediaDevices").unwrap();
    obj.set(scope, k.into(), media_devices.into());

    // connection (NetworkInformation)
    let connection = v8::Object::new(scope);
    set_str(scope, connection, "effectiveType", "4g");
    set_str(scope, connection, "type", "wifi");
    {
        let k = v8::String::new(scope, "downlink").unwrap();
        let v = v8::Number::new(scope, 10.0);
        connection.set(scope, k.into(), v.into());
        let k = v8::String::new(scope, "rtt").unwrap();
        let v = v8::Integer::new(scope, 50);
        connection.set(scope, k.into(), v.into());
    }
    set_bool(scope, connection, "saveData", false);
    let noop = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    let k = v8::String::new(scope, "addEventListener").unwrap();
    connection.set(scope, k.into(), noop.into());
    let k = v8::String::new(scope, "removeEventListener").unwrap();
    connection.set(scope, k.into(), noop.into());
    let k = v8::String::new(scope, "connection").unwrap();
    obj.set(scope, k.into(), connection.into());

    // geolocation
    let geolocation = v8::Object::new(scope);
    let geo_noop = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {
        log::trace!("navigator.geolocation method called (no-op in SSR)");
    }).unwrap();
    for name in &["getCurrentPosition", "watchPosition", "clearWatch"] {
        let k = v8::String::new(scope, name).unwrap();
        geolocation.set(scope, k.into(), geo_noop.into());
    }
    let k = v8::String::new(scope, "geolocation").unwrap();
    obj.set(scope, k.into(), geolocation.into());

    // maxTouchPoints
    {
        let k = v8::String::new(scope, "maxTouchPoints").unwrap();
        let v = v8::Integer::new(scope, 0);
        obj.set(scope, k.into(), v.into());
    }

    // storage (StorageManager)
    let storage = v8::Object::new(scope);
    let estimate_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let resolver = v8::PromiseResolver::new(scope).unwrap();
        let result = v8::Object::new(scope);
        let k = v8::String::new(scope, "quota").unwrap();
        let v = v8::Number::new(scope, 0.0);
        result.set(scope, k.into(), v.into());
        let k = v8::String::new(scope, "usage").unwrap();
        let v = v8::Number::new(scope, 0.0);
        result.set(scope, k.into(), v.into());
        resolver.resolve(scope, result.into());
        rv.set(resolver.get_promise(scope).into());
    }).unwrap();
    let k = v8::String::new(scope, "estimate").unwrap();
    storage.set(scope, k.into(), estimate_fn.into());
    let persist_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let resolver = v8::PromiseResolver::new(scope).unwrap();
        let val = v8::Boolean::new(scope, false);
        resolver.resolve(scope, val.into());
        rv.set(resolver.get_promise(scope).into());
    }).unwrap();
    let k = v8::String::new(scope, "persist").unwrap();
    storage.set(scope, k.into(), persist_fn.into());
    let k = v8::String::new(scope, "storage").unwrap();
    obj.set(scope, k.into(), storage.into());

    // credentials
    let credentials = v8::Object::new(scope);
    let cred_null_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let resolver = v8::PromiseResolver::new(scope).unwrap();
        let null = v8::null(scope);
        resolver.resolve(scope, null.into());
        rv.set(resolver.get_promise(scope).into());
    }).unwrap();
    for name in &["get", "store", "create"] {
        let k = v8::String::new(scope, name).unwrap();
        credentials.set(scope, k.into(), cred_null_fn.into());
    }
    let k = v8::String::new(scope, "credentials").unwrap();
    obj.set(scope, k.into(), credentials.into());

    // locks (Web Locks API)
    let locks = v8::Object::new(scope);
    let lock_noop = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    let k = v8::String::new(scope, "request").unwrap();
    locks.set(scope, k.into(), lock_noop.into());
    let k = v8::String::new(scope, "query").unwrap();
    locks.set(scope, k.into(), lock_noop.into());
    let k = v8::String::new(scope, "locks").unwrap();
    obj.set(scope, k.into(), locks.into());

    // Simple boolean/number/string properties
    set_bool(scope, obj, "webdriver", false);
    set_bool(scope, obj, "pdfViewerEnabled", true);
    {
        let k = v8::String::new(scope, "deviceMemory").unwrap();
        let v = v8::Number::new(scope, 8.0);
        obj.set(scope, k.into(), v.into());
    }

    // userAgentData
    let ua_data = v8::Object::new(scope);
    let brands = v8::Array::new(scope, 0);
    let k = v8::String::new(scope, "brands").unwrap();
    ua_data.set(scope, k.into(), brands.into());
    set_bool(scope, ua_data, "mobile", false);
    set_str(scope, ua_data, "platform", "Linux");
    let k = v8::String::new(scope, "userAgentData").unwrap();
    obj.set(scope, k.into(), ua_data.into());

    log::debug!("Installed navigator stubs (clipboard, permissions, mediaDevices, connection, geolocation, storage, credentials, locks, userAgentData)");

    obj
}

fn set_str(scope: &mut v8::HandleScope, obj: v8::Local<v8::Object>, key: &str, val: &str) {
    let k = v8::String::new(scope, key).unwrap();
    let v = v8::String::new(scope, val).unwrap();
    obj.set(scope, k.into(), v.into());
}

fn set_bool(scope: &mut v8::HandleScope, obj: v8::Local<v8::Object>, key: &str, val: bool) {
    let k = v8::String::new(scope, key).unwrap();
    let v = v8::Boolean::new(scope, val);
    obj.set(scope, k.into(), v.into());
}
