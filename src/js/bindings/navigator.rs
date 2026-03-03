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
