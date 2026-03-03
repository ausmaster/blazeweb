/// CustomElementRegistry (customElements global).

/// Install the `customElements` registry on the global object.
pub fn install(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    let custom_elements = create_custom_elements_registry(scope);
    let key = v8::String::new(scope, "customElements").unwrap();
    global.set(scope, key.into(), custom_elements.into());
}

fn create_custom_elements_registry<'s>(scope: &mut v8::HandleScope<'s>) -> v8::Local<'s, v8::Object> {
    let obj = v8::Object::new(scope);

    let map = v8::Object::new(scope);
    let pk = v8::String::new(scope, "__ceMap").unwrap();
    let hidden_key = v8::Private::for_api(scope, Some(pk));
    obj.set_private(scope, hidden_key, map.into());

    let define_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue| {
        let name = args.get(0).to_rust_string_lossy(scope);
        let ctor = args.get(1);
        let this = args.this();
        let pk = v8::String::new(scope, "__ceMap").unwrap();
        let hidden_key = v8::Private::for_api(scope, Some(pk));
        if let Some(map_val) = this.get_private(scope, hidden_key) {
            if let Ok(map) = v8::Local::<v8::Object>::try_from(map_val) {
                let k = v8::String::new(scope, &name).unwrap();
                map.set(scope, k.into(), ctor);
            }
        }
    }).unwrap();
    let k = v8::String::new(scope, "define").unwrap();
    obj.set(scope, k.into(), define_fn.into());

    let get_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let name = args.get(0).to_rust_string_lossy(scope);
        let this = args.this();
        let pk = v8::String::new(scope, "__ceMap").unwrap();
        let hidden_key = v8::Private::for_api(scope, Some(pk));
        if let Some(map_val) = this.get_private(scope, hidden_key) {
            if let Ok(map) = v8::Local::<v8::Object>::try_from(map_val) {
                let k = v8::String::new(scope, &name).unwrap();
                if let Some(val) = map.get(scope, k.into()) {
                    if !val.is_undefined() {
                        rv.set(val);
                        return;
                    }
                }
            }
        }
        rv.set(v8::undefined(scope).into());
    }).unwrap();
    let k = v8::String::new(scope, "get").unwrap();
    obj.set(scope, k.into(), get_fn.into());

    let when_defined = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let resolver = v8::PromiseResolver::new(scope).unwrap();
        let undef = v8::undefined(scope);
        resolver.resolve(scope, undef.into());
        rv.set(resolver.get_promise(scope).into());
    }).unwrap();
    let k = v8::String::new(scope, "whenDefined").unwrap();
    obj.set(scope, k.into(), when_defined.into());

    let noop = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    let k = v8::String::new(scope, "upgrade").unwrap();
    obj.set(scope, k.into(), noop.into());

    obj
}
