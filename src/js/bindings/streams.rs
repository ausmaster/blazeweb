//! ReadableStream Web API — minimum viable implementation for fetch().body.
//!
//! Since blazeweb fetches the full response body before JS runs,
//! the "stream" is a single-chunk delivery mechanism:
//! first reader.read() returns {value: data, done: false},
//! second reader.read() returns {value: undefined, done: true}.

/// Install ReadableStream constructor on the global object.
pub fn install(scope: &mut v8::PinnedRef<v8::HandleScope>, global: v8::Local<v8::Object>) {
    let rs_ctor = v8::Function::new(scope, readable_stream_constructor).unwrap();
    let key = v8::String::new(scope, "ReadableStream").unwrap();
    global.set(scope, key.into(), rs_ctor.into());
    log::debug!("Installed ReadableStream constructor");
}

/// Create a ReadableStream wrapping pre-fetched body bytes.
/// Used by fetch response to set response.body.
pub fn create_from_bytes<'s, 'i>(
    scope: &mut v8::PinnedRef<'s, v8::HandleScope<'i>>,
    body: &[u8],
) -> v8::Local<'s, v8::Object> {
    let obj = v8::Object::new(scope);

    // Store body bytes as a Uint8Array via private key
    let store = v8::ArrayBuffer::new(scope, body.len());
    {
        let backing = store.get_backing_store();
        let dest = &backing[..body.len()];
        // SAFETY: ArrayBuffer backing store is valid and correctly sized
        unsafe {
            std::ptr::copy_nonoverlapping(
                body.as_ptr(),
                dest.as_ptr() as *mut u8,
                body.len(),
            );
        }
    }
    let uint8 = v8::Uint8Array::new(scope, store, 0, body.len()).unwrap();
    let pk = v8::String::new(scope, "__streamData").unwrap();
    let private_key = v8::Private::for_api(scope, Some(pk));
    obj.set_private(scope, private_key, uint8.into());

    // locked property — starts as false, set to true by getReader()
    let k = v8::String::new(scope, "locked").unwrap();
    let false_val = v8::Boolean::new(scope, false);
    obj.set(scope, k.into(), false_val.into());

    // getReader() method
    let get_reader = v8::Function::builder(stream_get_reader)
        .build(scope)
        .unwrap();
    let k = v8::String::new(scope, "getReader").unwrap();
    obj.set(scope, k.into(), get_reader.into());

    // cancel() method — returns resolved promise
    let cancel_fn = v8::Function::new(scope, |scope: &mut v8::PinnedRef<v8::HandleScope>,
        _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let resolver = v8::PromiseResolver::new(scope).unwrap();
        let undef = v8::undefined(scope);
        resolver.resolve(scope, undef.into());
        rv.set(resolver.get_promise(scope).into());
    }).unwrap();
    let k = v8::String::new(scope, "cancel").unwrap();
    obj.set(scope, k.into(), cancel_fn.into());

    // pipeTo / pipeThrough / tee stubs
    let noop_promise = v8::Function::new(scope, |scope: &mut v8::PinnedRef<v8::HandleScope>,
        _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let resolver = v8::PromiseResolver::new(scope).unwrap();
        let undef = v8::undefined(scope);
        resolver.resolve(scope, undef.into());
        rv.set(resolver.get_promise(scope).into());
    }).unwrap();
    for name in &["pipeTo", "pipeThrough"] {
        let k = v8::String::new(scope, name).unwrap();
        obj.set(scope, k.into(), noop_promise.into());
    }
    let tee_fn = v8::Function::new(scope, |scope: &mut v8::PinnedRef<v8::HandleScope>,
        _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        rv.set(v8::Array::new(scope, 0).into());
    }).unwrap();
    let k = v8::String::new(scope, "tee").unwrap();
    obj.set(scope, k.into(), tee_fn.into());

    log::trace!("Created ReadableStream from {} bytes", body.len());
    obj
}

/// ReadableStream constructor: `new ReadableStream(underlyingSource?)`
fn readable_stream_constructor(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    // If called with an underlying source, extract start() callback
    let source = args.get(0);
    let has_source = source.is_object() && !source.is_null_or_undefined();

    // Create stream object (reuse create_from_bytes with empty for no-source)
    let obj = create_from_bytes(scope, &[]);

    // If underlying source provided, call start(controller)
    if has_source {
        let source_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(source) };
        let start_key = v8::String::new(scope, "start").unwrap();
        if let Some(start_val) = source_obj.get(scope, start_key.into())
            && start_val.is_function()
        {
            let start_fn = unsafe { v8::Local::<v8::Function>::cast_unchecked(start_val) };
            let controller = create_controller(scope, &obj);
            let undef = v8::undefined(scope);
            let args_arr: &[v8::Local<v8::Value>] = &[controller.into()];
            let _ = start_fn.call(scope, undef.into(), args_arr);
        }
    }

    rv.set(obj.into());
}

/// Create a ReadableStreamDefaultController for the start() callback
fn create_controller<'s, 'i>(
    scope: &mut v8::PinnedRef<'s, v8::HandleScope<'i>>,
    stream: &v8::Local<v8::Object>,
) -> v8::Local<'s, v8::Object> {
    let controller = v8::Object::new(scope);

    // Store reference to stream's data via private key
    let pk = v8::String::new(scope, "__stream").unwrap();
    let stream_key = v8::Private::for_api(scope, Some(pk));
    controller.set_private(scope, stream_key, (*stream).into());

    // enqueue(chunk)
    let enqueue_fn = v8::Function::new(scope, |scope: &mut v8::PinnedRef<v8::HandleScope>,
        args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue| {
        let this = args.this();
        let chunk = args.get(0);
        let pk = v8::String::new(scope, "__stream").unwrap();
        let stream_key = v8::Private::for_api(scope, Some(pk));
        if let Some(stream_val) = this.get_private(scope, stream_key)
            && stream_val.is_object()
        {
            let stream = unsafe { v8::Local::<v8::Object>::cast_unchecked(stream_val) };
            let pk = v8::String::new(scope, "__streamData").unwrap();
            let data_key = v8::Private::for_api(scope, Some(pk));
            stream.set_private(scope, data_key, chunk);
        }
    }).unwrap();
    let k = v8::String::new(scope, "enqueue").unwrap();
    controller.set(scope, k.into(), enqueue_fn.into());

    // close() — mark stream as closed
    let close_fn = v8::Function::new(scope, |scope: &mut v8::PinnedRef<v8::HandleScope>,
        args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue| {
        let this = args.this();
        let pk = v8::String::new(scope, "__stream").unwrap();
        let stream_key = v8::Private::for_api(scope, Some(pk));
        if let Some(stream_val) = this.get_private(scope, stream_key)
            && stream_val.is_object()
        {
            let stream = unsafe { v8::Local::<v8::Object>::cast_unchecked(stream_val) };
            let pk = v8::String::new(scope, "__closed").unwrap();
            let closed_key = v8::Private::for_api(scope, Some(pk));
            let true_val = v8::Boolean::new(scope, true);
            stream.set_private(scope, closed_key, true_val.into());
        }
    }).unwrap();
    let k = v8::String::new(scope, "close").unwrap();
    controller.set(scope, k.into(), close_fn.into());

    // error() — stub
    let error_fn = v8::Function::new(scope, |_: &mut v8::PinnedRef<v8::HandleScope>,
        _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {
    }).unwrap();
    let k = v8::String::new(scope, "error").unwrap();
    controller.set(scope, k.into(), error_fn.into());

    controller
}

/// Get locked state of stream
fn stream_locked_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    let pk = v8::String::new(scope, "__locked").unwrap();
    let locked_key = v8::Private::for_api(scope, Some(pk));
    if let Some(val) = this.get_private(scope, locked_key) {
        rv.set(val);
    } else {
        rv.set(v8::Boolean::new(scope, false).into());
    }
}

/// getReader() — returns a ReadableStreamDefaultReader
fn stream_get_reader(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();

    // Mark stream as locked (public property for JS access)
    let k = v8::String::new(scope, "locked").unwrap();
    let true_val = v8::Boolean::new(scope, true);
    this.set(scope, k.into(), true_val.into());

    // Create reader object
    let reader = v8::Object::new(scope);

    // Store stream reference on reader
    let pk = v8::String::new(scope, "__stream").unwrap();
    let stream_key = v8::Private::for_api(scope, Some(pk));
    reader.set_private(scope, stream_key, this.into());

    // __readDone flag (false initially)
    let pk = v8::String::new(scope, "__readDone").unwrap();
    let done_key = v8::Private::for_api(scope, Some(pk));
    let false_val = v8::Boolean::new(scope, false);
    reader.set_private(scope, done_key, false_val.into());

    // read() method — returns Promise<{value, done}>
    let read_fn = v8::Function::new(scope, reader_read).unwrap();
    let k = v8::String::new(scope, "read").unwrap();
    reader.set(scope, k.into(), read_fn.into());

    // cancel() — returns resolved promise
    let cancel_fn = v8::Function::new(scope, |scope: &mut v8::PinnedRef<v8::HandleScope>,
        _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let resolver = v8::PromiseResolver::new(scope).unwrap();
        let undef = v8::undefined(scope);
        resolver.resolve(scope, undef.into());
        rv.set(resolver.get_promise(scope).into());
    }).unwrap();
    let k = v8::String::new(scope, "cancel").unwrap();
    reader.set(scope, k.into(), cancel_fn.into());

    // releaseLock() — unlock the stream
    let release_fn = v8::Function::builder(reader_release_lock)
        .build(scope)
        .unwrap();
    let k = v8::String::new(scope, "releaseLock").unwrap();
    reader.set(scope, k.into(), release_fn.into());

    // closed — resolved promise
    let resolver = v8::PromiseResolver::new(scope).unwrap();
    let undef = v8::undefined(scope);
    resolver.resolve(scope, undef.into());
    let closed_promise = resolver.get_promise(scope);
    let k = v8::String::new(scope, "closed").unwrap();
    reader.set(scope, k.into(), closed_promise.into());

    log::trace!("ReadableStream.getReader() called");
    rv.set(reader.into());
}

/// reader.read() — first call returns {value: data, done: false},
/// subsequent calls return {value: undefined, done: true}
fn reader_read(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    let resolver = v8::PromiseResolver::new(scope).unwrap();

    // Check if already done
    let pk = v8::String::new(scope, "__readDone").unwrap();
    let done_key = v8::Private::for_api(scope, Some(pk));
    let already_done = this.get_private(scope, done_key)
        .map(|v| v.boolean_value(scope))
        .unwrap_or(false);

    let result = v8::Object::new(scope);

    if already_done {
        // Return {value: undefined, done: true}
        let k = v8::String::new(scope, "value").unwrap();
        let undef = v8::undefined(scope);
        result.set(scope, k.into(), undef.into());
        let k = v8::String::new(scope, "done").unwrap();
        let done = v8::Boolean::new(scope, true);
        result.set(scope, k.into(), done.into());
    } else {
        // Get stream data
        let pk = v8::String::new(scope, "__stream").unwrap();
        let stream_key = v8::Private::for_api(scope, Some(pk));
        let value = if let Some(stream_val) = this.get_private(scope, stream_key) {
            if stream_val.is_object() {
                let stream = unsafe { v8::Local::<v8::Object>::cast_unchecked(stream_val) };
                let pk = v8::String::new(scope, "__streamData").unwrap();
                let data_key = v8::Private::for_api(scope, Some(pk));
                stream.get_private(scope, data_key)
            } else {
                None
            }
        } else {
            None
        };

        let k = v8::String::new(scope, "value").unwrap();
        if let Some(val) = value {
            if !val.is_undefined() && !val.is_null() {
                result.set(scope, k.into(), val);
            } else {
                let undef = v8::undefined(scope);
                result.set(scope, k.into(), undef.into());
            }
        } else {
            let undef = v8::undefined(scope);
            result.set(scope, k.into(), undef.into());
        }
        let k = v8::String::new(scope, "done").unwrap();
        let done = v8::Boolean::new(scope, false);
        result.set(scope, k.into(), done.into());

        // Mark as done for next read
        let true_val = v8::Boolean::new(scope, true);
        this.set_private(scope, done_key, true_val.into());
    }

    resolver.resolve(scope, result.into());
    rv.set(resolver.get_promise(scope).into());
}

/// reader.releaseLock() — unlock the stream
fn reader_release_lock(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let this = args.this();
    let pk = v8::String::new(scope, "__stream").unwrap();
    let stream_key = v8::Private::for_api(scope, Some(pk));
    if let Some(stream_val) = this.get_private(scope, stream_key)
        && stream_val.is_object()
    {
        let stream = unsafe { v8::Local::<v8::Object>::cast_unchecked(stream_val) };
        let k = v8::String::new(scope, "locked").unwrap();
        let false_val = v8::Boolean::new(scope, false);
        stream.set(scope, k.into(), false_val.into());
    }
    log::trace!("ReadableStreamDefaultReader.releaseLock() called");
}
