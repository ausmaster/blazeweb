/// window.location — creates a Location-like object from the base_url.

/// Base URL stored in isolate slot.
pub struct BaseUrl(pub Option<String>);

/// Create a location object. If base_url is available, parse it;
/// otherwise provide empty-string defaults.
pub fn create_location_object<'s>(
    scope: &mut v8::HandleScope<'s>,
) -> v8::Local<'s, v8::Object> {
    let obj = v8::Object::new(scope);

    let base = scope.get_slot::<BaseUrl>()
        .map(|b| b.0.clone())
        .unwrap_or(None);

    let (href, protocol, host, hostname, port, pathname, search, hash, origin) = if let Some(ref url_str) = base {
        parse_url_parts(url_str)
    } else {
        (
            "about:blank".to_string(),
            "about:".to_string(),
            String::new(),
            String::new(),
            String::new(),
            "/".to_string(),
            String::new(),
            String::new(),
            "null".to_string(),
        )
    };

    set_str(scope, obj, "href", &href);
    set_str(scope, obj, "protocol", &protocol);
    set_str(scope, obj, "host", &host);
    set_str(scope, obj, "hostname", &hostname);
    set_str(scope, obj, "port", &port);
    set_str(scope, obj, "pathname", &pathname);
    set_str(scope, obj, "search", &search);
    set_str(scope, obj, "hash", &hash);
    set_str(scope, obj, "origin", &origin);

    // No-op methods
    let noop = v8::Function::new(scope, |_scope: &mut v8::HandleScope, _args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue| {}).unwrap();
    for name in &["assign", "replace", "reload"] {
        let k = v8::String::new(scope, name).unwrap();
        obj.set(scope, k.into(), noop.into());
    }

    // toString returns href — read it back from the object
    let to_string = v8::Function::new(scope, |scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let this = args.this();
        let k = v8::String::new(scope, "href").unwrap();
        if let Some(val) = this.get(scope, k.into()) {
            rv.set(val);
        }
    }).unwrap();
    let k = v8::String::new(scope, "toString").unwrap();
    obj.set(scope, k.into(), to_string.into());

    obj
}

fn set_str(scope: &mut v8::HandleScope, obj: v8::Local<v8::Object>, key: &str, val: &str) {
    let k = v8::String::new(scope, key).unwrap();
    let v = v8::String::new(scope, val).unwrap();
    obj.set(scope, k.into(), v.into());
}

pub fn parse_url_parts(url_str: &str) -> (String, String, String, String, String, String, String, String, String) {
    // Try to parse as a proper URL
    if let Ok(url) = reqwest::Url::parse(url_str) {
        let protocol = format!("{}:", url.scheme());
        let hostname = url.host_str().unwrap_or("").to_string();
        let port = url.port().map(|p| p.to_string()).unwrap_or_default();
        let host = if port.is_empty() {
            hostname.clone()
        } else {
            format!("{}:{}", hostname, port)
        };
        let pathname = url.path().to_string();
        let search = url.query().map(|q| format!("?{}", q)).unwrap_or_default();
        let hash = url.fragment().map(|f| format!("#{}", f)).unwrap_or_default();
        let origin = format!("{}//{}", protocol, host);

        (url.to_string(), protocol, host, hostname, port, pathname, search, hash, origin)
    } else {
        (
            url_str.to_string(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "/".to_string(),
            String::new(),
            String::new(),
            "null".to_string(),
        )
    }
}
