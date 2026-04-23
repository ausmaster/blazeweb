/// HTMLMediaElement prototype bindings.
///
/// Installed on the HTMLMediaElement FunctionTemplate prototype during
/// create_dom_templates(). Provides play/pause/load/canPlayType methods
/// and media property accessors shared by <video> and <audio> elements.

use super::helpers::{set_accessor, set_method};
use crate::js::templates::unwrap_node_id;

pub fn install(scope: &mut v8::PinnedRef<v8::HandleScope<()>>, proto: &v8::Local<v8::ObjectTemplate>) {
    // Methods
    set_method(scope, proto, "play", play);
    set_method(scope, proto, "pause", pause_noop);
    set_method(scope, proto, "load", load_noop);
    set_method(scope, proto, "canPlayType", can_play_type);

    // Read-only accessors returning SSR defaults
    set_accessor(scope, proto, "paused", paused_getter);
    set_accessor(scope, proto, "ended", ended_getter);
    set_accessor(scope, proto, "muted", muted_getter);
    set_accessor(scope, proto, "currentTime", current_time_getter);
    set_accessor(scope, proto, "duration", duration_getter);
    set_accessor(scope, proto, "volume", volume_getter);
    set_accessor(scope, proto, "playbackRate", playback_rate_getter);
    set_accessor(scope, proto, "readyState", ready_state_getter);
    set_accessor(scope, proto, "networkState", network_state_getter);
    set_accessor(scope, proto, "currentSrc", current_src_getter);
    set_accessor(scope, proto, "error", error_getter);
    set_accessor(scope, proto, "preload", preload_getter);
    set_accessor(scope, proto, "buffered", buffered_getter);
    set_accessor(scope, proto, "seekable", seekable_getter);
    set_accessor(scope, proto, "played", played_getter);
    set_accessor(scope, proto, "textTracks", text_tracks_getter);
}

// ─── Methods ─────────────────────────────────────────────────────────────────

fn play(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    log::trace!("HTMLMediaElement.play() called (SSR stub)");
    let resolver = v8::PromiseResolver::new(scope).unwrap();
    let undef = v8::undefined(scope);
    resolver.resolve(scope, undef.into());
    rv.set(resolver.get_promise(scope).into());
}

fn pause_noop(
    _scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {}

fn load_noop(
    _scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {}

fn can_play_type(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let empty = v8::String::new(scope, "").unwrap();
    rv.set(empty.into());
}

// ─── Boolean accessors ───────────────────────────────────────────────────────

fn paused_getter(scope: &mut v8::PinnedRef<v8::HandleScope>, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    rv.set(v8::Boolean::new(scope, true).into());
}
fn ended_getter(scope: &mut v8::PinnedRef<v8::HandleScope>, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    rv.set(v8::Boolean::new(scope, false).into());
}
fn muted_getter(scope: &mut v8::PinnedRef<v8::HandleScope>, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    rv.set(v8::Boolean::new(scope, false).into());
}

// ─── Number accessors ────────────────────────────────────────────────────────

fn current_time_getter(scope: &mut v8::PinnedRef<v8::HandleScope>, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    rv.set(v8::Number::new(scope, 0.0).into());
}
fn duration_getter(scope: &mut v8::PinnedRef<v8::HandleScope>, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    rv.set(v8::Number::new(scope, f64::NAN).into());
}
fn volume_getter(scope: &mut v8::PinnedRef<v8::HandleScope>, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    rv.set(v8::Number::new(scope, 1.0).into());
}
fn playback_rate_getter(scope: &mut v8::PinnedRef<v8::HandleScope>, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    rv.set(v8::Number::new(scope, 1.0).into());
}

// ─── Integer accessors ───────────────────────────────────────────────────────

fn ready_state_getter(scope: &mut v8::PinnedRef<v8::HandleScope>, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    rv.set(v8::Integer::new(scope, 0).into());
}
fn network_state_getter(scope: &mut v8::PinnedRef<v8::HandleScope>, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    rv.set(v8::Integer::new(scope, 0).into());
}

// ─── String accessors ────────────────────────────────────────────────────────

fn current_src_getter(scope: &mut v8::PinnedRef<v8::HandleScope>, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    // Read src attribute if element has one
    let _ = unwrap_node_id(scope, _args.this()); // needed for future real impl
    let empty = v8::String::new(scope, "").unwrap();
    rv.set(empty.into());
}
fn preload_getter(scope: &mut v8::PinnedRef<v8::HandleScope>, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let empty = v8::String::new(scope, "").unwrap();
    rv.set(empty.into());
}

// ─── Null accessor ───────────────────────────────────────────────────────────

fn error_getter(scope: &mut v8::PinnedRef<v8::HandleScope>, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    rv.set(v8::null(scope).into());
}

// ─── TimeRanges stubs ────────────────────────────────────────────────────────

fn buffered_getter(scope: &mut v8::PinnedRef<v8::HandleScope>, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    rv.set(make_time_ranges(scope).into());
}
fn seekable_getter(scope: &mut v8::PinnedRef<v8::HandleScope>, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    rv.set(make_time_ranges(scope).into());
}
fn played_getter(scope: &mut v8::PinnedRef<v8::HandleScope>, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    rv.set(make_time_ranges(scope).into());
}

fn make_time_ranges<'s, 'i>(scope: &mut v8::PinnedRef<'s, v8::HandleScope<'i>>) -> v8::Local<'s, v8::Object> {
    let tr = v8::Object::new(scope);
    let zero = v8::Integer::new(scope, 0);
    let k = v8::String::new(scope, "length").unwrap();
    tr.set(scope, k.into(), zero.into());
    // start/end throw IndexSizeError when called (length is 0)
    let throw_fn = v8::Function::new(scope, |scope: &mut v8::PinnedRef<v8::HandleScope>, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {
        let msg = v8::String::new(scope, "IndexSizeError").unwrap();
        let exc = v8::Exception::range_error(scope, msg);
        scope.throw_exception(exc);
    }).unwrap();
    let k = v8::String::new(scope, "start").unwrap();
    tr.set(scope, k.into(), throw_fn.into());
    let k = v8::String::new(scope, "end").unwrap();
    tr.set(scope, k.into(), throw_fn.into());
    tr
}

// ─── Array-like accessor ─────────────────────────────────────────────────────

fn text_tracks_getter(scope: &mut v8::PinnedRef<v8::HandleScope>, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let arr = v8::Array::new(scope, 0);
    rv.set(arr.into());
}
