/// AbortController constructor.

/// Install the AbortController constructor on the global object.
pub fn install(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    let ac_ctor = v8::Function::new(scope, abort_controller_constructor).unwrap();
    let key = v8::String::new(scope, "AbortController").unwrap();
    global.set(scope, key.into(), ac_ctor.into());
}

fn abort_controller_constructor(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let obj = v8::Object::new(scope);

    // signal object
    let signal = v8::Object::new(scope);
    let k = v8::String::new(scope, "aborted").unwrap();
    let v = v8::Boolean::new(scope, false);
    signal.set(scope, k.into(), v.into());
    let k = v8::String::new(scope, "reason").unwrap();
    let undef = v8::undefined(scope);
    signal.set(scope, k.into(), undef.into());
    // addEventListener/removeEventListener on signal
    let noop = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    let k = v8::String::new(scope, "addEventListener").unwrap();
    signal.set(scope, k.into(), noop.into());
    let k = v8::String::new(scope, "removeEventListener").unwrap();
    signal.set(scope, k.into(), noop.into());
    let k = v8::String::new(scope, "throwIfAborted").unwrap();
    signal.set(scope, k.into(), noop.into());

    let k = v8::String::new(scope, "signal").unwrap();
    obj.set(scope, k.into(), signal.into());

    // Store signal as private for abort()
    let pk_name = v8::String::new(scope, "__signal").unwrap();
    let hidden_key = v8::Private::for_api(scope, Some(pk_name));
    obj.set_private(scope, hidden_key, signal.into());

    // abort() method
    let abort_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue| {
        let this = args.this();
        let pk_name = v8::String::new(scope, "__signal").unwrap();
        let hidden_key = v8::Private::for_api(scope, Some(pk_name));
        if let Some(sig_val) = this.get_private(scope, hidden_key) {
            if let Ok(signal) = v8::Local::<v8::Object>::try_from(sig_val) {
                let k = v8::String::new(scope, "aborted").unwrap();
                let v = v8::Boolean::new(scope, true);
                signal.set(scope, k.into(), v.into());
            }
        }
    }).unwrap();
    let k = v8::String::new(scope, "abort").unwrap();
    obj.set(scope, k.into(), abort_fn.into());

    rv.set(obj.into());
}
