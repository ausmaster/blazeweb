/// MessageChannel, MessagePort, and Worker constructors.

/// Install messaging constructors on the global object.
pub fn install(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    let mc_ctor = v8::Function::new(scope, message_channel_constructor).unwrap();
    let key = v8::String::new(scope, "MessageChannel").unwrap();
    global.set(scope, key.into(), mc_ctor.into());

    let worker_ctor = v8::Function::new(scope, worker_constructor).unwrap();
    let key = v8::String::new(scope, "Worker").unwrap();
    global.set(scope, key.into(), worker_ctor.into());

    let sw_ctor = v8::Function::new(scope, shared_worker_constructor).unwrap();
    let key = v8::String::new(scope, "SharedWorker").unwrap();
    global.set(scope, key.into(), sw_ctor.into());
    log::debug!("Installed SharedWorker constructor");
}

fn message_channel_constructor(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let obj = v8::Object::new(scope);
    let port1 = create_message_port(scope);
    let port2 = create_message_port(scope);
    let k = v8::String::new(scope, "port1").unwrap();
    obj.set(scope, k.into(), port1.into());
    let k = v8::String::new(scope, "port2").unwrap();
    obj.set(scope, k.into(), port2.into());
    rv.set(obj.into());
}

fn create_message_port<'s>(scope: &mut v8::HandleScope<'s>) -> v8::Local<'s, v8::Object> {
    let port = v8::Object::new(scope);
    let null = v8::null(scope);
    let k = v8::String::new(scope, "onmessage").unwrap();
    port.set(scope, k.into(), null.into());
    let k = v8::String::new(scope, "onmessageerror").unwrap();
    port.set(scope, k.into(), null.into());
    let noop = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    for name in &["postMessage", "close", "start", "addEventListener", "removeEventListener"] {
        let k = v8::String::new(scope, name).unwrap();
        port.set(scope, k.into(), noop.into());
    }
    port
}

fn worker_constructor(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let obj = v8::Object::new(scope);
    let null = v8::null(scope);
    for name in &["onmessage", "onerror", "onmessageerror"] {
        let k = v8::String::new(scope, name).unwrap();
        obj.set(scope, k.into(), null.into());
    }
    let noop = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    for name in &["postMessage", "terminate", "addEventListener", "removeEventListener"] {
        let k = v8::String::new(scope, name).unwrap();
        obj.set(scope, k.into(), noop.into());
    }
    rv.set(obj.into());
}

fn shared_worker_constructor(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let obj = v8::Object::new(scope);
    let port = create_message_port(scope);
    let k = v8::String::new(scope, "port").unwrap();
    obj.set(scope, k.into(), port.into());
    let null = v8::null(scope);
    let k = v8::String::new(scope, "onerror").unwrap();
    obj.set(scope, k.into(), null.into());
    let noop = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    for name in &["addEventListener", "removeEventListener"] {
        let k = v8::String::new(scope, name).unwrap();
        obj.set(scope, k.into(), noop.into());
    }
    rv.set(obj.into());
}
