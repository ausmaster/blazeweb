/// Observer constructors: MutationObserver (real), IntersectionObserver (fires callbacks),
/// ResizeObserver (fires callbacks), PerformanceObserver (real).

use crate::dom::arena::NodeId;
use crate::js::mutation_observer::{MutationObserverState, ObserverOptions};
use crate::js::templates::unwrap_node_id;

/// Install observer constructors on the global object.
pub fn install(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    let mo = v8::Function::new(scope, mutation_observer_constructor).unwrap();
    let key = v8::String::new(scope, "MutationObserver").unwrap();
    global.set(scope, key.into(), mo.into());

    let io = v8::Function::new(scope, intersection_observer_constructor).unwrap();
    let key = v8::String::new(scope, "IntersectionObserver").unwrap();
    global.set(scope, key.into(), io.into());

    let ro = v8::Function::new(scope, resize_observer_constructor).unwrap();
    let key = v8::String::new(scope, "ResizeObserver").unwrap();
    global.set(scope, key.into(), ro.into());

    let po = v8::Function::new(scope, performance_observer_constructor).unwrap();
    // PerformanceObserver.supportedEntryTypes (static property)
    let supported = v8::Array::new(scope, 2);
    let mark_str = v8::String::new(scope, "mark").unwrap();
    let measure_str = v8::String::new(scope, "measure").unwrap();
    supported.set_index(scope, 0, mark_str.into());
    supported.set_index(scope, 1, measure_str.into());
    // Freeze the array so it's read-only per spec
    let k = v8::String::new(scope, "supportedEntryTypes").unwrap();
    po.set(scope, k.into(), supported.into());
    let key = v8::String::new(scope, "PerformanceObserver").unwrap();
    global.set(scope, key.into(), po.into());
}

// ─── MutationObserver (real implementation) ──────────────────────────────────

fn mutation_observer_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let callback_arg = args.get(0);
    if !callback_arg.is_function() {
        let msg = v8::String::new(
            scope,
            "Failed to construct 'MutationObserver': The callback provided as parameter 1 is not a function.",
        ).unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    }
    let callback = unsafe { v8::Local::<v8::Function>::cast_unchecked(callback_arg) };
    let callback_global = v8::Global::new(scope, callback);

    // Create the JS observer object
    let obj = v8::Object::new(scope);
    let obj_global = v8::Global::new(scope, obj);

    // Register in centralized state
    let observer_idx = {
        let state = scope.get_slot_mut::<MutationObserverState>().unwrap();
        state.add_observer(callback_global, obj_global)
    };

    // Store observer_idx as private property
    let name = v8::String::new(scope, "__mo_idx").unwrap();
    let idx_key = v8::Private::for_api(scope, Some(name));
    let idx_val = v8::Integer::new(scope, observer_idx as i32);
    obj.set_private(scope, idx_key, idx_val.into());

    // observe(target, options)
    let observe_fn = v8::Function::new(scope, mo_observe).unwrap();
    let k = v8::String::new(scope, "observe").unwrap();
    obj.set(scope, k.into(), observe_fn.into());

    // disconnect()
    let disconnect_fn = v8::Function::new(scope, mo_disconnect).unwrap();
    let k = v8::String::new(scope, "disconnect").unwrap();
    obj.set(scope, k.into(), disconnect_fn.into());

    // takeRecords()
    let take_fn = v8::Function::new(scope, mo_take_records).unwrap();
    let k = v8::String::new(scope, "takeRecords").unwrap();
    obj.set(scope, k.into(), take_fn.into());

    rv.set(obj.into());
}

/// Extract observer index from a MO JS object via private property.
fn get_observer_idx(scope: &mut v8::HandleScope, this: v8::Local<v8::Object>) -> Option<usize> {
    let name = v8::String::new(scope, "__mo_idx").unwrap();
    let idx_key = v8::Private::for_api(scope, Some(name));
    let val = this.get_private(scope, idx_key)?;
    Some(val.int32_value(scope)? as usize)
}

fn mo_observe(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let this = args.this();
    let Some(observer_idx) = get_observer_idx(scope, this) else { return };

    // Arg 0: target node
    let target_arg = args.get(0);
    if !target_arg.is_object() {
        let msg = v8::String::new(scope, "Failed to execute 'observe' on 'MutationObserver': parameter 1 is not of type 'Node'.").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    }
    let target_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(target_arg) };
    let Some(target_id) = unwrap_node_id(scope, target_obj) else {
        let msg = v8::String::new(scope, "Failed to execute 'observe' on 'MutationObserver': parameter 1 is not of type 'Node'.").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    };

    // Arg 1: options dict
    let opts_obj = if args.length() > 1 && args.get(1).is_object() {
        Some(unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) })
    } else {
        None
    };

    // Parse options with explicit-vs-default tracking per spec
    let (
        child_list,
        attributes_explicit,
        character_data_explicit,
        subtree,
        attribute_old_value,
        character_data_old_value,
        attribute_filter,
    ) = if let Some(obj) = opts_obj {
        (
            get_bool_opt(scope, obj, "childList").unwrap_or(false),
            get_bool_opt(scope, obj, "attributes"),
            get_bool_opt(scope, obj, "characterData"),
            get_bool_opt(scope, obj, "subtree").unwrap_or(false),
            get_bool_opt(scope, obj, "attributeOldValue").unwrap_or(false),
            get_bool_opt(scope, obj, "characterDataOldValue").unwrap_or(false),
            get_string_array_opt(scope, obj, "attributeFilter"),
        )
    } else {
        (false, None, None, false, false, false, None)
    };

    let mut attributes = attributes_explicit.unwrap_or(false);
    let mut character_data = character_data_explicit.unwrap_or(false);

    // Spec step 1: if attributeOldValue or attributeFilter is set but attributes
    // was not explicitly provided, set attributes = true.
    if (attribute_old_value || attribute_filter.is_some()) && attributes_explicit.is_none() {
        attributes = true;
    }

    // Spec step 2: if characterDataOldValue is set but characterData was not
    // explicitly provided, set characterData = true.
    if character_data_old_value && character_data_explicit.is_none() {
        character_data = true;
    }

    // Spec step 3: at least one of childList/attributes/characterData must be true.
    if !child_list && !attributes && !character_data {
        let msg = v8::String::new(scope, "Failed to execute 'observe' on 'MutationObserver': The options object must set at least one of 'attributes', 'characterData', or 'childList' to true.").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    }

    let options = ObserverOptions {
        child_list,
        attributes,
        character_data,
        subtree,
        attribute_old_value,
        character_data_old_value,
        attribute_filter: attribute_filter.unwrap_or_default(),
    };

    let state = scope.get_slot_mut::<MutationObserverState>().unwrap();
    state.observe(observer_idx, target_id, options);
}

fn mo_disconnect(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let this = args.this();
    let Some(observer_idx) = get_observer_idx(scope, this) else { return };
    let state = scope.get_slot_mut::<MutationObserverState>().unwrap();
    state.disconnect(observer_idx);
}

fn mo_take_records(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    let Some(observer_idx) = get_observer_idx(scope, this) else {
        rv.set(v8::Array::new(scope, 0).into());
        return;
    };
    let arr = crate::js::mutation_observer::build_records_array(scope, observer_idx);
    rv.set(arr.into());
}

// ─── Option parsing helpers ──────────────────────────────────────────────────

fn get_bool_opt(
    scope: &mut v8::HandleScope,
    obj: v8::Local<v8::Object>,
    name: &str,
) -> Option<bool> {
    let k = v8::String::new(scope, name).unwrap();
    let v = obj.get(scope, k.into())?;
    if v.is_undefined() {
        return None;
    }
    Some(v.boolean_value(scope))
}

fn get_string_array_opt(
    scope: &mut v8::HandleScope,
    obj: v8::Local<v8::Object>,
    name: &str,
) -> Option<Vec<String>> {
    let k = v8::String::new(scope, name).unwrap();
    let v = obj.get(scope, k.into())?;
    if v.is_undefined() {
        return None;
    }
    if !v.is_array() {
        return Some(Vec::new());
    }
    let arr = unsafe { v8::Local::<v8::Array>::cast_unchecked(v) };
    let mut result = Vec::new();
    for i in 0..arr.length() {
        if let Some(elem) = arr.get_index(scope, i) {
            result.push(elem.to_rust_string_lossy(scope));
        }
    }
    Some(result)
}

// ─── IntersectionObserver (fires callbacks with all entries visible) ──────────

/// Per-observer state for IntersectionObserver.
struct IntersectionObserverEntry {
    callback: v8::Global<v8::Function>,
    observer_obj: v8::Global<v8::Object>,
    targets: Vec<NodeId>,
    fired: bool,
}

/// Isolate-slot state tracking all IntersectionObserver instances.
pub struct IntersectionObserverState {
    observers: Vec<IntersectionObserverEntry>,
}

impl IntersectionObserverState {
    pub fn new() -> Self {
        Self {
            observers: Vec::new(),
        }
    }
}

/// Fire all pending IntersectionObserver callbacks.
/// Each observed target is reported as fully visible (isIntersecting: true, ratio: 1.0).
/// Returns any JS errors from callback execution.
pub fn drain_intersection_observers(scope: &mut v8::HandleScope) -> Vec<String> {
    let mut errors = Vec::new();

    // Collect pending observers (callback + targets) — must release slot borrow first
    let pending: Vec<(v8::Global<v8::Function>, v8::Global<v8::Object>, Vec<NodeId>)> = {
        let Some(state) = scope.get_slot_mut::<IntersectionObserverState>() else {
            return errors;
        };
        state
            .observers
            .iter_mut()
            .filter(|o| !o.fired && !o.targets.is_empty())
            .map(|o| {
                o.fired = true;
                (o.callback.clone(), o.observer_obj.clone(), o.targets.clone())
            })
            .collect()
    };

    if pending.is_empty() {
        log::trace!("no pending IntersectionObserver callbacks");
        return errors;
    }

    let total_targets: usize = pending.iter().map(|(_, _, t)| t.len()).sum();
    log::info!(
        "firing {} IntersectionObserver callback(s) with {} total entries",
        pending.len(),
        total_targets,
    );

    for (callback_global, observer_global, targets) in &pending {
        let callback = v8::Local::new(scope, callback_global);
        let observer = v8::Local::new(scope, observer_global);

        // Build entries array — one IntersectionObserverEntry per target
        let entries = v8::Array::new(scope, targets.len() as i32);
        for (i, node_id) in targets.iter().enumerate() {
            let entry = build_io_entry(scope, *node_id);
            entries.set_index(scope, i as u32, entry.into());
        }
        log::debug!(
            "IntersectionObserver callback: {} entries (real layout geometry)",
            targets.len(),
        );

        // Call callback(entries, observer)
        let try_catch = &mut v8::TryCatch::new(scope);
        let undefined = v8::undefined(try_catch);
        let args: &[v8::Local<v8::Value>] = &[entries.into(), observer.into()];
        if callback.call(try_catch, undefined.into(), args).is_none() {
            if let Some(exc) = try_catch.exception() {
                let msg = exc.to_rust_string_lossy(try_catch);
                log::warn!("IntersectionObserver callback error: {}", msg);
                errors.push(msg);
            }
        }
    }

    log::info!(
        "IntersectionObserver drain complete: {} callbacks fired, {} errors",
        pending.len(),
        errors.len(),
    );
    errors
}

/// Build a single IntersectionObserverEntry for a target node.
/// Uses real layout data from Taffy to compute bounding rect and intersection.
fn build_io_entry<'s>(scope: &mut v8::HandleScope<'s>, node_id: NodeId) -> v8::Local<'s, v8::Object> {
    let arena = crate::js::templates::arena_ref(scope);
    let layout = &arena.nodes[node_id].taffy_layout;
    let (abs_x, abs_y) = arena.absolute_position(node_id);

    let bounding_x = abs_x as f64;
    let bounding_y = abs_y as f64;
    let bounding_w = layout.size.width as f64;
    let bounding_h = layout.size.height as f64;

    // Viewport is 0,0 → 1920,1080
    const VP_W: f64 = 1920.0;
    const VP_H: f64 = 1080.0;

    // Intersection = clip bounding rect against viewport
    let ix = bounding_x.max(0.0);
    let iy = bounding_y.max(0.0);
    let ix2 = (bounding_x + bounding_w).min(VP_W);
    let iy2 = (bounding_y + bounding_h).min(VP_H);
    let iw = (ix2 - ix).max(0.0);
    let ih = (iy2 - iy).max(0.0);

    let bounding_area = bounding_w * bounding_h;
    let intersection_area = iw * ih;
    let ratio = if bounding_area > 0.0 {
        (intersection_area / bounding_area).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let is_intersecting = intersection_area > 0.0;

    log::trace!(
        "IO entry {:?}: bounding=({:.0},{:.0} {:.0}x{:.0}) intersecting={} ratio={:.2}",
        node_id, bounding_x, bounding_y, bounding_w, bounding_h, is_intersecting, ratio
    );

    let entry = v8::Object::new(scope);

    // target
    let target = crate::js::templates::wrap_node(scope, node_id);
    let k = v8::String::new(scope, "target").unwrap();
    entry.set(scope, k.into(), target.into());

    // isIntersecting
    let k = v8::String::new(scope, "isIntersecting").unwrap();
    let v = v8::Boolean::new(scope, is_intersecting);
    entry.set(scope, k.into(), v.into());

    // intersectionRatio
    let k = v8::String::new(scope, "intersectionRatio").unwrap();
    let v = v8::Number::new(scope, ratio);
    entry.set(scope, k.into(), v.into());

    // time
    let k = v8::String::new(scope, "time").unwrap();
    let v = v8::Number::new(scope, 0.0);
    entry.set(scope, k.into(), v.into());

    // boundingClientRect — real element bounds
    let bounding_rect = make_dom_rect(scope, bounding_x, bounding_y, bounding_w, bounding_h);
    let k = v8::String::new(scope, "boundingClientRect").unwrap();
    entry.set(scope, k.into(), bounding_rect.into());

    // intersectionRect — clipped against viewport
    let intersection_rect = make_dom_rect(scope, ix, iy, iw, ih);
    let k = v8::String::new(scope, "intersectionRect").unwrap();
    entry.set(scope, k.into(), intersection_rect.into());

    // rootBounds — viewport
    let root_bounds = make_dom_rect(scope, 0.0, 0.0, VP_W, VP_H);
    let k = v8::String::new(scope, "rootBounds").unwrap();
    entry.set(scope, k.into(), root_bounds.into());

    entry
}

/// Create a DOMRectReadOnly-like object.
fn make_dom_rect<'s>(
    scope: &mut v8::HandleScope<'s>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> v8::Local<'s, v8::Object> {
    let rect = v8::Object::new(scope);
    for (name, val) in &[
        ("x", x),
        ("y", y),
        ("width", width),
        ("height", height),
        ("top", y),
        ("right", x + width),
        ("bottom", y + height),
        ("left", x),
    ] {
        let k = v8::String::new(scope, name).unwrap();
        let v = v8::Number::new(scope, *val);
        rect.set(scope, k.into(), v.into());
    }
    rect
}

fn intersection_observer_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    // Arg 0: callback (required)
    let callback_arg = args.get(0);
    if !callback_arg.is_function() {
        let msg = v8::String::new(
            scope,
            "Failed to construct 'IntersectionObserver': The callback provided as parameter 1 is not a function.",
        ).unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    }
    let callback = unsafe { v8::Local::<v8::Function>::cast_unchecked(callback_arg) };
    let callback_global = v8::Global::new(scope, callback);

    let obj = v8::Object::new(scope);
    let obj_global = v8::Global::new(scope, obj);

    // Register observer in state
    let observer_idx = {
        let state = scope.get_slot_mut::<IntersectionObserverState>().unwrap();
        let idx = state.observers.len();
        state.observers.push(IntersectionObserverEntry {
            callback: callback_global,
            observer_obj: obj_global,
            targets: Vec::new(),
            fired: false,
        });
        idx
    };
    log::debug!("IntersectionObserver created (idx={})", observer_idx);

    // Store observer index as private property
    let name = v8::String::new(scope, "__io_idx").unwrap();
    let idx_key = v8::Private::for_api(scope, Some(name));
    let idx_val = v8::Integer::new(scope, observer_idx as i32);
    obj.set_private(scope, idx_key, idx_val.into());

    // observe(target)
    let observe_fn = v8::Function::new(scope, io_observe).unwrap();
    let k = v8::String::new(scope, "observe").unwrap();
    obj.set(scope, k.into(), observe_fn.into());

    // unobserve(target)
    let unobserve_fn = v8::Function::new(scope, io_unobserve).unwrap();
    let k = v8::String::new(scope, "unobserve").unwrap();
    obj.set(scope, k.into(), unobserve_fn.into());

    // disconnect()
    let disconnect_fn = v8::Function::new(scope, io_disconnect).unwrap();
    let k = v8::String::new(scope, "disconnect").unwrap();
    obj.set(scope, k.into(), disconnect_fn.into());

    // takeRecords()
    let take_fn = v8::Function::new(scope, io_take_records).unwrap();
    let k = v8::String::new(scope, "takeRecords").unwrap();
    obj.set(scope, k.into(), take_fn.into());

    // Read-only properties
    let k = v8::String::new(scope, "root").unwrap();
    let val = v8::null(scope);
    obj.set(scope, k.into(), val.into());

    let k = v8::String::new(scope, "rootMargin").unwrap();
    let v = v8::String::new(scope, "0px 0px 0px 0px").unwrap();
    obj.set(scope, k.into(), v.into());

    // Parse thresholds from options
    let thresholds = if args.length() > 1 && args.get(1).is_object() {
        let opts = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        let t_key = v8::String::new(scope, "threshold").unwrap();
        if let Some(t_val) = opts.get(scope, t_key.into()) {
            if t_val.is_array() {
                unsafe { v8::Local::<v8::Array>::cast_unchecked(t_val) }
            } else if t_val.is_number() {
                let arr = v8::Array::new(scope, 1);
                arr.set_index(scope, 0, t_val);
                arr
            } else {
                let arr = v8::Array::new(scope, 1);
                let zero = v8::Number::new(scope, 0.0);
                arr.set_index(scope, 0, zero.into());
                arr
            }
        } else {
            let arr = v8::Array::new(scope, 1);
            let zero = v8::Number::new(scope, 0.0);
            arr.set_index(scope, 0, zero.into());
            arr
        }
    } else {
        let arr = v8::Array::new(scope, 1);
        let zero = v8::Number::new(scope, 0.0);
        arr.set_index(scope, 0, zero.into());
        arr
    };
    let k = v8::String::new(scope, "thresholds").unwrap();
    obj.set(scope, k.into(), thresholds.into());

    rv.set(obj.into());
}

fn get_io_idx(scope: &mut v8::HandleScope, this: v8::Local<v8::Object>) -> Option<usize> {
    let name = v8::String::new(scope, "__io_idx").unwrap();
    let idx_key = v8::Private::for_api(scope, Some(name));
    let val = this.get_private(scope, idx_key)?;
    Some(val.int32_value(scope)? as usize)
}

fn io_observe(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let this = args.this();
    let Some(observer_idx) = get_io_idx(scope, this) else { return };

    let target_arg = args.get(0);
    if !target_arg.is_object() { return; }
    let target_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(target_arg) };
    let Some(node_id) = unwrap_node_id(scope, target_obj) else { return };

    let state = scope.get_slot_mut::<IntersectionObserverState>().unwrap();
    if let Some(entry) = state.observers.get_mut(observer_idx) {
        if !entry.targets.contains(&node_id) {
            log::debug!("IntersectionObserver[{}].observe({:?}), now {} targets", observer_idx, node_id, entry.targets.len() + 1);
            entry.targets.push(node_id);
        }
    }
}

fn io_unobserve(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let this = args.this();
    let Some(observer_idx) = get_io_idx(scope, this) else { return };

    let target_arg = args.get(0);
    if !target_arg.is_object() { return; }
    let target_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(target_arg) };
    let Some(node_id) = unwrap_node_id(scope, target_obj) else { return };

    let state = scope.get_slot_mut::<IntersectionObserverState>().unwrap();
    if let Some(entry) = state.observers.get_mut(observer_idx) {
        entry.targets.retain(|&id| id != node_id);
    }
}

fn io_disconnect(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let this = args.this();
    let Some(observer_idx) = get_io_idx(scope, this) else { return };
    let state = scope.get_slot_mut::<IntersectionObserverState>().unwrap();
    if let Some(entry) = state.observers.get_mut(observer_idx) {
        log::debug!("IntersectionObserver[{}].disconnect(), had {} targets", observer_idx, entry.targets.len());
        entry.targets.clear();
        entry.fired = true; // prevent future firing
    }
}

fn io_take_records(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    rv.set(v8::Array::new(scope, 0).into());
}

// ─── ResizeObserver (fires callbacks with observed entries) ──────────────────

/// Per-observer state for ResizeObserver.
struct ResizeObserverEntry {
    callback: v8::Global<v8::Function>,
    observer_obj: v8::Global<v8::Object>,
    targets: Vec<NodeId>,
    fired: bool,
}

/// Isolate-slot state tracking all ResizeObserver instances.
pub struct ResizeObserverState {
    observers: Vec<ResizeObserverEntry>,
}

impl ResizeObserverState {
    pub fn new() -> Self {
        Self {
            observers: Vec::new(),
        }
    }
}

/// Fire all pending ResizeObserver callbacks.
/// Each observed target is reported with a default 1920x1080 content box.
pub fn drain_resize_observers(scope: &mut v8::HandleScope) -> Vec<String> {
    let mut errors = Vec::new();

    let pending: Vec<(v8::Global<v8::Function>, v8::Global<v8::Object>, Vec<NodeId>)> = {
        let Some(state) = scope.get_slot_mut::<ResizeObserverState>() else {
            return errors;
        };
        state
            .observers
            .iter_mut()
            .filter(|o| !o.fired && !o.targets.is_empty())
            .map(|o| {
                o.fired = true;
                (o.callback.clone(), o.observer_obj.clone(), o.targets.clone())
            })
            .collect()
    };

    if pending.is_empty() {
        log::trace!("no pending ResizeObserver callbacks");
        return errors;
    }

    let total_targets: usize = pending.iter().map(|(_, _, t)| t.len()).sum();
    log::info!(
        "firing {} ResizeObserver callback(s) with {} total entries",
        pending.len(),
        total_targets,
    );

    for (callback_global, observer_global, targets) in &pending {
        let callback = v8::Local::new(scope, callback_global);
        let observer = v8::Local::new(scope, observer_global);

        let entries = v8::Array::new(scope, targets.len() as i32);
        for (i, node_id) in targets.iter().enumerate() {
            let entry = build_ro_entry(scope, *node_id);
            entries.set_index(scope, i as u32, entry.into());
        }
        log::debug!("ResizeObserver callback: {} entries (real layout geometry)", targets.len());

        let try_catch = &mut v8::TryCatch::new(scope);
        let undefined = v8::undefined(try_catch);
        let args: &[v8::Local<v8::Value>] = &[entries.into(), observer.into()];
        if callback.call(try_catch, undefined.into(), args).is_none() {
            if let Some(exc) = try_catch.exception() {
                let msg = exc.to_rust_string_lossy(try_catch);
                log::warn!("ResizeObserver callback error: {}", msg);
                errors.push(msg);
            }
        }
    }

    log::info!(
        "ResizeObserver drain complete: {} callbacks fired, {} errors",
        pending.len(),
        errors.len(),
    );
    errors
}

/// Build a single ResizeObserverEntry for a target node.
/// Uses real layout data from Taffy for content/border box dimensions.
fn build_ro_entry<'s>(scope: &mut v8::HandleScope<'s>, node_id: NodeId) -> v8::Local<'s, v8::Object> {
    let arena = crate::js::templates::arena_ref(scope);
    let layout = &arena.nodes[node_id].taffy_layout;

    // Border box = full element size
    let border_w = layout.size.width as f64;
    let border_h = layout.size.height as f64;

    // Content box = size minus padding and border
    let content_x = (layout.padding.left + layout.border.left) as f64;
    let content_y = (layout.padding.top + layout.border.top) as f64;
    let content_w = (border_w - content_x - (layout.padding.right + layout.border.right) as f64).max(0.0);
    let content_h = (border_h - content_y - (layout.padding.bottom + layout.border.bottom) as f64).max(0.0);

    log::trace!(
        "RO entry {:?}: border={:.0}x{:.0} content={:.0}x{:.0}",
        node_id, border_w, border_h, content_w, content_h
    );

    let entry = v8::Object::new(scope);

    // target
    let target = crate::js::templates::wrap_node(scope, node_id);
    let k = v8::String::new(scope, "target").unwrap();
    entry.set(scope, k.into(), target.into());

    // contentRect (DOMRectReadOnly) — content box relative to padding edge
    let rect = make_dom_rect(scope, content_x, content_y, content_w, content_h);
    let k = v8::String::new(scope, "contentRect").unwrap();
    entry.set(scope, k.into(), rect.into());

    // contentBoxSize array with one ResizeObserverSize
    let content_size = make_ro_size(scope, content_w, content_h);
    let arr = v8::Array::new(scope, 1);
    arr.set_index(scope, 0, content_size.into());
    let k = v8::String::new(scope, "contentBoxSize").unwrap();
    entry.set(scope, k.into(), arr.into());

    // borderBoxSize array
    let border_size = make_ro_size(scope, border_w, border_h);
    let arr2 = v8::Array::new(scope, 1);
    arr2.set_index(scope, 0, border_size.into());
    let k = v8::String::new(scope, "borderBoxSize").unwrap();
    entry.set(scope, k.into(), arr2.into());

    // devicePixelContentBoxSize (same as content at 1x DPR)
    let device_size = make_ro_size(scope, content_w, content_h);
    let arr3 = v8::Array::new(scope, 1);
    arr3.set_index(scope, 0, device_size.into());
    let k = v8::String::new(scope, "devicePixelContentBoxSize").unwrap();
    entry.set(scope, k.into(), arr3.into());

    entry
}

/// Create a ResizeObserverSize object with inlineSize and blockSize.
fn make_ro_size<'s>(
    scope: &mut v8::HandleScope<'s>,
    inline_size: f64,
    block_size: f64,
) -> v8::Local<'s, v8::Object> {
    let size = v8::Object::new(scope);
    let k = v8::String::new(scope, "inlineSize").unwrap();
    let v = v8::Number::new(scope, inline_size);
    size.set(scope, k.into(), v.into());
    let k = v8::String::new(scope, "blockSize").unwrap();
    let v = v8::Number::new(scope, block_size);
    size.set(scope, k.into(), v.into());
    size
}

fn resize_observer_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let callback_arg = args.get(0);
    if !callback_arg.is_function() {
        let msg = v8::String::new(
            scope,
            "Failed to construct 'ResizeObserver': The callback provided as parameter 1 is not a function.",
        ).unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    }
    let callback = unsafe { v8::Local::<v8::Function>::cast_unchecked(callback_arg) };
    let callback_global = v8::Global::new(scope, callback);

    let obj = v8::Object::new(scope);
    let obj_global = v8::Global::new(scope, obj);

    let observer_idx = {
        let state = scope.get_slot_mut::<ResizeObserverState>().unwrap();
        let idx = state.observers.len();
        state.observers.push(ResizeObserverEntry {
            callback: callback_global,
            observer_obj: obj_global,
            targets: Vec::new(),
            fired: false,
        });
        idx
    };
    log::debug!("ResizeObserver created (idx={})", observer_idx);

    let name = v8::String::new(scope, "__ro_idx").unwrap();
    let idx_key = v8::Private::for_api(scope, Some(name));
    let idx_val = v8::Integer::new(scope, observer_idx as i32);
    obj.set_private(scope, idx_key, idx_val.into());

    let observe_fn = v8::Function::new(scope, ro_observe).unwrap();
    let k = v8::String::new(scope, "observe").unwrap();
    obj.set(scope, k.into(), observe_fn.into());

    let unobserve_fn = v8::Function::new(scope, ro_unobserve).unwrap();
    let k = v8::String::new(scope, "unobserve").unwrap();
    obj.set(scope, k.into(), unobserve_fn.into());

    let disconnect_fn = v8::Function::new(scope, ro_disconnect).unwrap();
    let k = v8::String::new(scope, "disconnect").unwrap();
    obj.set(scope, k.into(), disconnect_fn.into());

    rv.set(obj.into());
}

fn get_ro_idx(scope: &mut v8::HandleScope, this: v8::Local<v8::Object>) -> Option<usize> {
    let name = v8::String::new(scope, "__ro_idx").unwrap();
    let idx_key = v8::Private::for_api(scope, Some(name));
    let val = this.get_private(scope, idx_key)?;
    Some(val.int32_value(scope)? as usize)
}

fn ro_observe(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let this = args.this();
    let Some(observer_idx) = get_ro_idx(scope, this) else { return };

    let target_arg = args.get(0);
    if !target_arg.is_object() { return; }
    let target_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(target_arg) };
    let Some(node_id) = unwrap_node_id(scope, target_obj) else { return };

    let state = scope.get_slot_mut::<ResizeObserverState>().unwrap();
    if let Some(entry) = state.observers.get_mut(observer_idx) {
        if !entry.targets.contains(&node_id) {
            log::debug!("ResizeObserver[{}].observe({:?}), now {} targets", observer_idx, node_id, entry.targets.len() + 1);
            entry.targets.push(node_id);
        }
    }
}

fn ro_unobserve(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let this = args.this();
    let Some(observer_idx) = get_ro_idx(scope, this) else { return };

    let target_arg = args.get(0);
    if !target_arg.is_object() { return; }
    let target_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(target_arg) };
    let Some(node_id) = unwrap_node_id(scope, target_obj) else { return };

    let state = scope.get_slot_mut::<ResizeObserverState>().unwrap();
    if let Some(entry) = state.observers.get_mut(observer_idx) {
        entry.targets.retain(|&id| id != node_id);
    }
}

fn ro_disconnect(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let this = args.this();
    let Some(observer_idx) = get_ro_idx(scope, this) else { return };
    let state = scope.get_slot_mut::<ResizeObserverState>().unwrap();
    if let Some(entry) = state.observers.get_mut(observer_idx) {
        log::debug!("ResizeObserver[{}].disconnect(), had {} targets", observer_idx, entry.targets.len());
        entry.targets.clear();
        entry.fired = true;
    }
}

// ─── PerformanceObserver (real implementation) ──────────────────────────────

/// A performance entry (mark or measure).
#[derive(Clone, Debug)]
pub struct PerformanceEntry {
    pub name: String,
    pub entry_type: String,
    pub start_time: f64,
    pub duration: f64,
}

impl PerformanceEntry {
    /// Filter a slice of entries by type.
    pub fn filter_by_type(entries: &[PerformanceEntry], entry_type: &str) -> Vec<PerformanceEntry> {
        entries.iter().filter(|e| e.entry_type == entry_type).cloned().collect()
    }

    /// Filter a slice of entries by name and optional type.
    pub fn filter_by_name(entries: &[PerformanceEntry], name: &str, entry_type: Option<&str>) -> Vec<PerformanceEntry> {
        entries.iter().filter(|e| {
            e.name == name && entry_type.map_or(true, |t| e.entry_type == t)
        }).cloned().collect()
    }
}

/// Per-observer state for PerformanceObserver.
struct PerformanceObserverData {
    callback: v8::Global<v8::Function>,
    observer_obj: v8::Global<v8::Object>,
    /// Entry types being observed (multi-type mode).
    entry_types: Vec<String>,
    /// Pending entries to deliver to callback.
    pending_entries: Vec<PerformanceEntry>,
    /// Whether this observer is connected (not disconnected).
    connected: bool,
}

/// Isolate-slot state tracking all PerformanceObserver instances
/// and the global performance timeline buffer.
pub struct PerformanceObserverState {
    observers: Vec<PerformanceObserverData>,
    /// Global performance timeline buffer (all marks/measures).
    timeline: Vec<PerformanceEntry>,
    /// Named marks for performance.measure() startMark/endMark lookup.
    marks: Vec<PerformanceEntry>,
}

impl PerformanceObserverState {
    pub fn new() -> Self {
        Self {
            observers: Vec::new(),
            timeline: Vec::new(),
            marks: Vec::new(),
        }
    }

    /// Add a performance entry to the timeline and queue it to matching observers.
    pub fn add_entry(&mut self, entry: PerformanceEntry) {
        log::debug!("performance entry: type={}, name={}, startTime={}, duration={}",
            entry.entry_type, entry.name, entry.start_time, entry.duration);
        self.timeline.push(entry.clone());
        if entry.entry_type == "mark" {
            self.marks.push(entry.clone());
        }
        for observer in &mut self.observers {
            if observer.connected && observer.entry_types.contains(&entry.entry_type) {
                observer.pending_entries.push(entry.clone());
            }
        }
    }

    /// Look up a named mark's start_time.
    pub fn get_mark_time(&self, name: &str) -> Option<f64> {
        // Per spec, return the most recent mark with this name
        self.marks.iter().rev().find(|e| e.name == name).map(|e| e.start_time)
    }

    /// Get all timeline entries.
    pub fn get_timeline(&self) -> &[PerformanceEntry] {
        &self.timeline
    }

    /// Clear marks from timeline (optionally by name).
    pub fn clear_marks(&mut self, name: Option<&str>) {
        if let Some(name) = name {
            self.timeline.retain(|e| !(e.entry_type == "mark" && e.name == name));
            self.marks.retain(|e| e.name != name);
        } else {
            self.timeline.retain(|e| e.entry_type != "mark");
            self.marks.clear();
        }
    }

    /// Clear measures from timeline (optionally by name).
    pub fn clear_measures(&mut self, name: Option<&str>) {
        if let Some(name) = name {
            self.timeline.retain(|e| !(e.entry_type == "measure" && e.name == name));
        } else {
            self.timeline.retain(|e| e.entry_type != "measure");
        }
    }
}

/// Fire all pending PerformanceObserver callbacks.
pub fn drain_performance_observers(scope: &mut v8::HandleScope) -> Vec<String> {
    let mut errors = Vec::new();

    let pending: Vec<(usize, v8::Global<v8::Function>, v8::Global<v8::Object>, Vec<PerformanceEntry>)> = {
        let Some(state) = scope.get_slot_mut::<PerformanceObserverState>() else {
            return errors;
        };
        state
            .observers
            .iter_mut()
            .enumerate()
            .filter(|(_, o)| o.connected && !o.pending_entries.is_empty())
            .map(|(i, o)| {
                let entries = std::mem::take(&mut o.pending_entries);
                (i, o.callback.clone(), o.observer_obj.clone(), entries)
            })
            .collect()
    };

    if pending.is_empty() {
        return errors;
    }

    log::info!("firing {} PerformanceObserver callback(s)", pending.len());

    for (_idx, callback_global, observer_global, entries) in &pending {
        let callback = v8::Local::new(scope, callback_global);
        let observer = v8::Local::new(scope, observer_global);

        // Build PerformanceObserverEntryList
        let entry_list = build_performance_entry_list(scope, entries);

        let try_catch = &mut v8::TryCatch::new(scope);
        let undefined = v8::undefined(try_catch);
        let args: &[v8::Local<v8::Value>] = &[entry_list.into(), observer.into()];
        if callback.call(try_catch, undefined.into(), args).is_none() {
            if let Some(exc) = try_catch.exception() {
                let msg = exc.to_rust_string_lossy(try_catch);
                log::warn!("PerformanceObserver callback error: {}", msg);
                errors.push(msg);
            }
        }
    }

    errors
}

/// Build a PerformanceObserverEntryList JS object from entries.
fn build_performance_entry_list<'s>(
    scope: &mut v8::HandleScope<'s>,
    entries: &[PerformanceEntry],
) -> v8::Local<'s, v8::Object> {
    let list = v8::Object::new(scope);

    // Store entries as a JS array in a private property for method access
    let entries_arr = build_performance_entries_array(scope, entries);
    let priv_name = v8::String::new(scope, "__po_entries").unwrap();
    let priv_key = v8::Private::for_api(scope, Some(priv_name));
    list.set_private(scope, priv_key, entries_arr.into());

    // getEntries()
    let get_entries = v8::Function::new(scope, po_list_get_entries).unwrap();
    let k = v8::String::new(scope, "getEntries").unwrap();
    list.set(scope, k.into(), get_entries.into());

    // getEntriesByType(type)
    let get_by_type = v8::Function::new(scope, po_list_get_entries_by_type).unwrap();
    let k = v8::String::new(scope, "getEntriesByType").unwrap();
    list.set(scope, k.into(), get_by_type.into());

    // getEntriesByName(name, type?)
    let get_by_name = v8::Function::new(scope, po_list_get_entries_by_name).unwrap();
    let k = v8::String::new(scope, "getEntriesByName").unwrap();
    list.set(scope, k.into(), get_by_name.into());

    list
}

/// Build an array of PerformanceEntry JS objects.
pub fn build_performance_entries_array<'s>(
    scope: &mut v8::HandleScope<'s>,
    entries: &[PerformanceEntry],
) -> v8::Local<'s, v8::Array> {
    let arr = v8::Array::new(scope, entries.len() as i32);
    for (i, entry) in entries.iter().enumerate() {
        let obj = build_performance_entry_obj(scope, entry);
        arr.set_index(scope, i as u32, obj.into());
    }
    arr
}

/// Build a single PerformanceEntry JS object.
fn build_performance_entry_obj<'s>(
    scope: &mut v8::HandleScope<'s>,
    entry: &PerformanceEntry,
) -> v8::Local<'s, v8::Object> {
    let obj = v8::Object::new(scope);

    let k = v8::String::new(scope, "name").unwrap();
    let v = v8::String::new(scope, &entry.name).unwrap();
    obj.set(scope, k.into(), v.into());

    let k = v8::String::new(scope, "entryType").unwrap();
    let v = v8::String::new(scope, &entry.entry_type).unwrap();
    obj.set(scope, k.into(), v.into());

    let k = v8::String::new(scope, "startTime").unwrap();
    let v = v8::Number::new(scope, entry.start_time);
    obj.set(scope, k.into(), v.into());

    let k = v8::String::new(scope, "duration").unwrap();
    let v = v8::Number::new(scope, entry.duration);
    obj.set(scope, k.into(), v.into());

    // toJSON()
    let to_json = v8::Function::new(scope, |scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let this = args.this();
        let result = v8::Object::new(scope);
        for prop_name in &["name", "entryType", "startTime", "duration"] {
            let k = v8::String::new(scope, prop_name).unwrap();
            if let Some(val) = this.get(scope, k.into()) {
                result.set(scope, k.into(), val);
            }
        }
        rv.set(result.into());
    }).unwrap();
    let k = v8::String::new(scope, "toJSON").unwrap();
    obj.set(scope, k.into(), to_json.into());

    obj
}

/// Get the private entries array from a PerformanceObserverEntryList object.
fn get_po_entries<'s>(
    scope: &mut v8::HandleScope<'s>,
    list: v8::Local<v8::Object>,
) -> Option<v8::Local<'s, v8::Array>> {
    let priv_name = v8::String::new(scope, "__po_entries").unwrap();
    let priv_key = v8::Private::for_api(scope, Some(priv_name));
    let val = list.get_private(scope, priv_key)?;
    if val.is_array() {
        Some(unsafe { v8::Local::<v8::Array>::cast_unchecked(val) })
    } else {
        None
    }
}

fn po_list_get_entries(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    if let Some(arr) = get_po_entries(scope, this) {
        rv.set(arr.into());
    } else {
        rv.set(v8::Array::new(scope, 0).into());
    }
}

fn po_list_get_entries_by_type(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    let type_str = args.get(0).to_rust_string_lossy(scope);
    let Some(arr) = get_po_entries(scope, this) else {
        rv.set(v8::Array::new(scope, 0).into());
        return;
    };
    let mut filtered = Vec::new();
    for i in 0..arr.length() {
        if let Some(entry) = arr.get_index(scope, i) {
            if entry.is_object() {
                let obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(entry) };
                let k = v8::String::new(scope, "entryType").unwrap();
                if let Some(et) = obj.get(scope, k.into()) {
                    if et.to_rust_string_lossy(scope) == type_str {
                        filtered.push(entry);
                    }
                }
            }
        }
    }
    let result = v8::Array::new(scope, filtered.len() as i32);
    for (i, entry) in filtered.iter().enumerate() {
        result.set_index(scope, i as u32, *entry);
    }
    rv.set(result.into());
}

fn po_list_get_entries_by_name(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    let name_str = args.get(0).to_rust_string_lossy(scope);
    let type_filter = if args.length() > 1 && !args.get(1).is_undefined() {
        Some(args.get(1).to_rust_string_lossy(scope))
    } else {
        None
    };
    let Some(arr) = get_po_entries(scope, this) else {
        rv.set(v8::Array::new(scope, 0).into());
        return;
    };
    let mut filtered = Vec::new();
    for i in 0..arr.length() {
        if let Some(entry) = arr.get_index(scope, i) {
            if entry.is_object() {
                let obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(entry) };
                let k = v8::String::new(scope, "name").unwrap();
                if let Some(n) = obj.get(scope, k.into()) {
                    if n.to_rust_string_lossy(scope) != name_str { continue; }
                }
                if let Some(ref tf) = type_filter {
                    let k = v8::String::new(scope, "entryType").unwrap();
                    if let Some(et) = obj.get(scope, k.into()) {
                        if et.to_rust_string_lossy(scope) != *tf { continue; }
                    }
                }
                filtered.push(entry);
            }
        }
    }
    let result = v8::Array::new(scope, filtered.len() as i32);
    for (i, entry) in filtered.iter().enumerate() {
        result.set_index(scope, i as u32, *entry);
    }
    rv.set(result.into());
}

fn performance_observer_constructor(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let callback_arg = args.get(0);
    if !callback_arg.is_function() {
        let msg = v8::String::new(
            scope,
            "Failed to construct 'PerformanceObserver': The callback provided as parameter 1 is not a function.",
        ).unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    }
    let callback = unsafe { v8::Local::<v8::Function>::cast_unchecked(callback_arg) };
    let callback_global = v8::Global::new(scope, callback);

    let obj = v8::Object::new(scope);
    let obj_global = v8::Global::new(scope, obj);

    let observer_idx = {
        let state = scope.get_slot_mut::<PerformanceObserverState>().unwrap();
        let idx = state.observers.len();
        state.observers.push(PerformanceObserverData {
            callback: callback_global,
            observer_obj: obj_global,
            entry_types: Vec::new(),
            pending_entries: Vec::new(),
            connected: false,
        });
        idx
    };
    log::debug!("PerformanceObserver created (idx={})", observer_idx);

    let name = v8::String::new(scope, "__po_idx").unwrap();
    let idx_key = v8::Private::for_api(scope, Some(name));
    let idx_val = v8::Integer::new(scope, observer_idx as i32);
    obj.set_private(scope, idx_key, idx_val.into());

    // observe(options)
    let observe_fn = v8::Function::new(scope, po_observe).unwrap();
    let k = v8::String::new(scope, "observe").unwrap();
    obj.set(scope, k.into(), observe_fn.into());

    // disconnect()
    let disconnect_fn = v8::Function::new(scope, po_disconnect).unwrap();
    let k = v8::String::new(scope, "disconnect").unwrap();
    obj.set(scope, k.into(), disconnect_fn.into());

    // takeRecords()
    let take_fn = v8::Function::new(scope, po_take_records).unwrap();
    let k = v8::String::new(scope, "takeRecords").unwrap();
    obj.set(scope, k.into(), take_fn.into());

    rv.set(obj.into());
}

fn get_po_idx(scope: &mut v8::HandleScope, this: v8::Local<v8::Object>) -> Option<usize> {
    let name = v8::String::new(scope, "__po_idx").unwrap();
    let idx_key = v8::Private::for_api(scope, Some(name));
    let val = this.get_private(scope, idx_key)?;
    Some(val.int32_value(scope)? as usize)
}

fn po_observe(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let this = args.this();
    let Some(observer_idx) = get_po_idx(scope, this) else { return };

    let opts_arg = args.get(0);
    if !opts_arg.is_object() {
        let msg = v8::String::new(scope, "Failed to execute 'observe' on 'PerformanceObserver': 1 argument required, but only 0 present.").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    }
    let opts = unsafe { v8::Local::<v8::Object>::cast_unchecked(opts_arg) };

    // Check for entryTypes (multi-type mode) or type (single-type mode)
    let entry_types_key = v8::String::new(scope, "entryTypes").unwrap();
    let type_key = v8::String::new(scope, "type").unwrap();

    let entry_types_val = opts.get(scope, entry_types_key.into());
    let type_val = opts.get(scope, type_key.into());

    let mut entry_types: Vec<String> = Vec::new();
    let mut buffered = false;

    let has_entry_types = entry_types_val.map_or(false, |v| !v.is_undefined() && !v.is_null());
    if has_entry_types {
        let et_val = entry_types_val.unwrap();
        if et_val.is_array() {
            // Multi-type observer mode: observe({entryTypes: [...]})
            let arr = unsafe { v8::Local::<v8::Array>::cast_unchecked(et_val) };
            for i in 0..arr.length() {
                if let Some(elem) = arr.get_index(scope, i) {
                    let s = elem.to_rust_string_lossy(scope);
                    if !s.is_empty() {
                        entry_types.push(s);
                    }
                }
            }
        }
    } else if let Some(t_val) = type_val {
        if !t_val.is_undefined() && !t_val.is_null() {
            // Single-type observer mode: observe({type: "...", buffered: true/false})
            entry_types.push(t_val.to_rust_string_lossy(scope));
            let buffered_key = v8::String::new(scope, "buffered").unwrap();
            if let Some(b_val) = opts.get(scope, buffered_key.into()) {
                buffered = b_val.boolean_value(scope);
            }
        }
    }

    if entry_types.is_empty() {
        let msg = v8::String::new(scope, "Failed to execute 'observe' on 'PerformanceObserver': An observe() call must include either entryTypes or type arguments.").unwrap();
        let exc = v8::Exception::type_error(scope, msg);
        scope.throw_exception(exc);
        return;
    }

    log::debug!("PerformanceObserver[{}].observe(entryTypes={:?}, buffered={})", observer_idx, entry_types, buffered);

    let state = scope.get_slot_mut::<PerformanceObserverState>().unwrap();
    if let Some(observer) = state.observers.get_mut(observer_idx) {
        observer.entry_types = entry_types.clone();
        observer.connected = true;

        // If buffered, deliver existing timeline entries matching the requested types
        if buffered {
            let existing: Vec<PerformanceEntry> = state.timeline.iter()
                .filter(|e| entry_types.contains(&e.entry_type))
                .cloned()
                .collect();
            observer.pending_entries.extend(existing);
        }
    }
}

fn po_disconnect(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let this = args.this();
    let Some(observer_idx) = get_po_idx(scope, this) else { return };
    let state = scope.get_slot_mut::<PerformanceObserverState>().unwrap();
    if let Some(observer) = state.observers.get_mut(observer_idx) {
        log::debug!("PerformanceObserver[{}].disconnect()", observer_idx);
        observer.connected = false;
        observer.entry_types.clear();
        observer.pending_entries.clear();
    }
}

fn po_take_records(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    let Some(observer_idx) = get_po_idx(scope, this) else {
        rv.set(v8::Array::new(scope, 0).into());
        return;
    };
    let entries = {
        let state = scope.get_slot_mut::<PerformanceObserverState>().unwrap();
        if let Some(observer) = state.observers.get_mut(observer_idx) {
            std::mem::take(&mut observer.pending_entries)
        } else {
            Vec::new()
        }
    };
    let arr = build_performance_entries_array(scope, &entries);
    rv.set(arr.into());
}

#[cfg(test)]
#[path = "observers_tests.rs"]
mod tests;
