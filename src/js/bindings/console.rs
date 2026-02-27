/// Console API: console.log, .warn, .error, .info
///
/// All methods are no-ops by default (output goes to log::debug).

pub fn install(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    let console = v8::Object::new(scope);

    set_method(scope, console, "log", console_log);
    set_method(scope, console, "warn", console_warn);
    set_method(scope, console, "error", console_error);
    set_method(scope, console, "info", console_log);
    set_method(scope, console, "debug", console_log);

    let key = v8::String::new(scope, "console").unwrap();
    global.set(scope, key.into(), console.into());
}

fn set_method(
    scope: &mut v8::HandleScope,
    obj: v8::Local<v8::Object>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
) {
    let key = v8::String::new(scope, name).unwrap();
    let func = v8::Function::new(scope, callback).unwrap();
    obj.set(scope, key.into(), func.into());
}

fn console_log(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let msg = format_args_to_string(scope, &args);
    log::debug!("[console.log] {msg}");
}

fn console_warn(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let msg = format_args_to_string(scope, &args);
    log::warn!("[console.warn] {msg}");
}

fn console_error(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let msg = format_args_to_string(scope, &args);
    log::error!("[console.error] {msg}");
}

fn format_args_to_string(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
) -> String {
    let mut parts = Vec::with_capacity(args.length() as usize);
    for i in 0..args.length() {
        let val = args.get(i);
        parts.push(val.to_rust_string_lossy(scope));
    }
    parts.join(" ")
}
