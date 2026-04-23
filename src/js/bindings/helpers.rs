/// Shared helper functions for installing V8 accessors and methods on ObjectTemplates.
///
/// Deduplicates the identical `set_accessor`, `set_accessor_with_setter`, and `set_method`
/// functions that were previously copy-pasted across element.rs, document.rs, node.rs, etc.

pub fn set_accessor(
    scope: &mut v8::PinnedRef<v8::HandleScope<()>>,
    proto: &v8::Local<v8::ObjectTemplate>,
    name: &str,
    getter: impl v8::MapFnTo<v8::FunctionCallback>,
) {
    let key = v8::String::new(scope, name).unwrap();
    let getter_ft = v8::FunctionTemplate::new(scope, getter);
    proto.set_accessor_property(key.into(), Some(getter_ft), None, v8::PropertyAttribute::NONE);
}

pub fn set_accessor_with_setter(
    scope: &mut v8::PinnedRef<v8::HandleScope<()>>,
    proto: &v8::Local<v8::ObjectTemplate>,
    name: &str,
    getter: impl v8::MapFnTo<v8::FunctionCallback>,
    setter: impl v8::MapFnTo<v8::FunctionCallback>,
) {
    let key = v8::String::new(scope, name).unwrap();
    let getter_ft = v8::FunctionTemplate::new(scope, getter);
    let setter_ft = v8::FunctionTemplate::new(scope, setter);
    proto.set_accessor_property(key.into(), Some(getter_ft), Some(setter_ft), v8::PropertyAttribute::NONE);
}

pub fn set_method(
    scope: &mut v8::PinnedRef<v8::HandleScope<()>>,
    proto: &v8::Local<v8::ObjectTemplate>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
) {
    let key = v8::String::new(scope, name).unwrap();
    let ft = v8::FunctionTemplate::new(scope, callback);
    proto.set(key.into(), ft.into());
}
