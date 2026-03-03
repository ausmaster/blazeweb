//! Timer APIs: setTimeout, setInterval, clearTimeout, clearInterval,
//! requestAnimationFrame, cancelAnimationFrame.
//!
//! SSR semantics: timers are collected during script execution, then drained
//! after all scripts have run. Intervals fire once. Callbacks execute in
//! delay-ascending order.

use std::collections::BTreeMap;

/// A pending timer callback.
pub(crate) struct TimerEntry {
    pub callback: v8::Global<v8::Function>,
    pub delay_ms: u64,
    pub _is_interval: bool,
}

/// Queue of pending timers, stored as an isolate slot.
///
/// Made `pub` so runtime.rs can set it as an isolate slot.
pub struct TimerQueue {
    pub(crate) timers: BTreeMap<u32, TimerEntry>,
    pub(crate) next_id: u32,
}

impl TimerQueue {
    pub fn new() -> Self {
        Self {
            timers: BTreeMap::new(),
            next_id: 1,
        }
    }

    pub fn add(&mut self, callback: v8::Global<v8::Function>, delay_ms: u64, is_interval: bool) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.timers.insert(id, TimerEntry {
            callback,
            delay_ms,
            _is_interval: is_interval,
        });
        id
    }

    pub fn remove(&mut self, id: u32) {
        self.timers.remove(&id);
    }

    pub fn is_empty(&self) -> bool {
        self.timers.is_empty()
    }
}

/// Install timer functions on the global object.
pub fn install(scope: &mut v8::HandleScope) {
    let context = scope.get_current_context();
    let global = context.global(scope);

    macro_rules! set_fn {
        ($name:expr, $cb:ident) => {{
            let key = v8::String::new(scope, $name).unwrap();
            let func = v8::Function::new(scope, $cb).unwrap();
            global.set(scope, key.into(), func.into());
        }};
    }

    set_fn!("setTimeout", set_timeout);
    set_fn!("setInterval", set_interval);
    set_fn!("clearTimeout", clear_timeout);
    set_fn!("clearInterval", clear_interval);
    set_fn!("requestAnimationFrame", request_animation_frame);
    // cancelAnimationFrame uses same impl as clearTimeout
    let key = v8::String::new(scope, "cancelAnimationFrame").unwrap();
    let func = v8::Function::new(scope, clear_timeout).unwrap();
    global.set(scope, key.into(), func.into());
}

/// Drain all pending timers, executing callbacks in delay order.
/// Returns collected error messages. Re-drains up to `max_rounds` times
/// in case timer callbacks schedule new timers.
pub fn drain(scope: &mut v8::HandleScope, max_rounds: usize) -> Vec<String> {
    let mut errors = Vec::new();

    for _ in 0..max_rounds {
        // Extract all timers sorted by (delay, id)
        let queue = scope.get_slot_mut::<TimerQueue>().unwrap();
        if queue.timers.is_empty() {
            break;
        }

        // Sort by delay, then by id for stable ordering
        let mut entries: Vec<(u32, u64, v8::Global<v8::Function>)> = std::mem::take(&mut queue.timers)
            .into_iter()
            .map(|(id, entry)| (id, entry.delay_ms, entry.callback))
            .collect();
        entries.sort_by_key(|(id, delay, _)| (*delay, *id));

        for (_id, _delay, callback) in entries {
            let try_catch = &mut v8::TryCatch::new(scope);
            let func: v8::Local<v8::Function> = v8::Local::new(try_catch, &callback);
            let undefined = v8::undefined(try_catch);
            if func.call(try_catch, undefined.into(), &[]).is_none() {
                if let Some(exc) = try_catch.exception() {
                    errors.push(exc.to_rust_string_lossy(try_catch));
                }
            }
        }
    }

    errors
}

// ─── Callbacks ───────────────────────────────────────────────────────────────

fn set_timeout(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let id = add_timer(scope, &args, false);
    rv.set(v8::Integer::new(scope, id as i32).into());
}

fn set_interval(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let id = add_timer(scope, &args, true);
    rv.set(v8::Integer::new(scope, id as i32).into());
}

fn clear_timeout(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let id = args.get(0).int32_value(scope).unwrap_or(0) as u32;
    if let Some(queue) = scope.get_slot_mut::<TimerQueue>() {
        queue.remove(id);
    }
}

fn clear_interval(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let id = args.get(0).int32_value(scope).unwrap_or(0) as u32;
    if let Some(queue) = scope.get_slot_mut::<TimerQueue>() {
        queue.remove(id);
    }
}

fn request_animation_frame(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    // rAF is essentially setTimeout(fn, 0) for SSR
    let id = add_timer(scope, &args, false);
    rv.set(v8::Integer::new(scope, id as i32).into());
}

fn add_timer(scope: &mut v8::HandleScope, args: &v8::FunctionCallbackArguments, is_interval: bool) -> u32 {
    let callback_arg = args.get(0);
    if !callback_arg.is_function() {
        return 0;
    }
    let func = unsafe { v8::Local::<v8::Function>::cast_unchecked(callback_arg) };
    let global_func = v8::Global::new(scope, func);

    let delay = if args.length() > 1 {
        args.get(1).int32_value(scope).unwrap_or(0).max(0) as u64
    } else {
        0
    };

    let queue = scope.get_slot_mut::<TimerQueue>().unwrap();
    queue.add(global_func, delay, is_interval)
}
