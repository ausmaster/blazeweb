/// MutationObserver centralized state and notification helpers.
///
/// Architecture: All MO state lives in a single `MutationObserverState` stored
/// in the V8 isolate slot. This avoids per-node storage overhead (99%+ of nodes
/// are never observed). The `registrations` map provides O(1) lookup during
/// inclusive-ancestor walks.
///
/// Ported from Servo's `components/script/dom/mutationobserver.rs`.
/// https://dom.spec.whatwg.org/#interface-mutationobserver

use std::collections::HashMap;

use crate::dom::arena::NodeId;
use crate::js::templates::{arena_ref, wrap_node};

// ─── Core types ──────────────────────────────────────────────────────────────

/// Centralized MutationObserver state, stored in V8 isolate slot.
pub struct MutationObserverState {
    observers: Vec<ObserverEntry>,
    registrations: HashMap<NodeId, Vec<Registration>>,
    microtask_queued: bool,
}

struct ObserverEntry {
    callback: v8::Global<v8::Function>,
    js_object: v8::Global<v8::Object>,
    record_queue: Vec<MutationRecordData>,
    observed_nodes: Vec<NodeId>,
}

#[derive(Clone)]
struct Registration {
    observer_idx: usize,
    options: ObserverOptions,
}

#[derive(Clone, Default)]
pub struct ObserverOptions {
    pub child_list: bool,
    pub attributes: bool,
    pub character_data: bool,
    pub subtree: bool,
    pub attribute_old_value: bool,
    pub character_data_old_value: bool,
    pub attribute_filter: Vec<String>,
}

#[derive(Clone)]
pub(crate) struct MutationRecordData {
    mutation_type: &'static str,
    target: NodeId,
    added_nodes: Vec<NodeId>,
    removed_nodes: Vec<NodeId>,
    previous_sibling: Option<NodeId>,
    next_sibling: Option<NodeId>,
    attribute_name: Option<String>,
    attribute_namespace: Option<String>,
    old_value: Option<String>,
}

// ─── MutationObserverState impl ──────────────────────────────────────────────

impl MutationObserverState {
    pub fn new() -> Self {
        Self {
            observers: Vec::new(),
            registrations: HashMap::new(),
            microtask_queued: false,
        }
    }

    pub fn has_observers(&self) -> bool {
        !self.observers.is_empty()
    }

    /// Register a new observer. Returns its index.
    pub fn add_observer(
        &mut self,
        callback: v8::Global<v8::Function>,
        js_object: v8::Global<v8::Object>,
    ) -> usize {
        let idx = self.observers.len();
        self.observers.push(ObserverEntry {
            callback,
            js_object,
            record_queue: Vec::new(),
            observed_nodes: Vec::new(),
        });
        idx
    }

    /// Register an observation. If the observer already watches this target,
    /// replace its options (per spec step 7).
    pub fn observe(&mut self, observer_idx: usize, target: NodeId, options: ObserverOptions) {
        let regs = self.registrations.entry(target).or_default();
        if let Some(reg) = regs.iter_mut().find(|r| r.observer_idx == observer_idx) {
            reg.options = options;
        } else {
            regs.push(Registration { observer_idx, options });
            self.observers[observer_idx].observed_nodes.push(target);
        }
    }

    /// Remove all registrations for this observer (spec: MutationObserver.disconnect).
    pub fn disconnect(&mut self, observer_idx: usize) {
        if observer_idx >= self.observers.len() {
            return;
        }
        let nodes = std::mem::take(&mut self.observers[observer_idx].observed_nodes);
        for node_id in nodes {
            if let Some(regs) = self.registrations.get_mut(&node_id) {
                regs.retain(|r| r.observer_idx != observer_idx);
                if regs.is_empty() {
                    self.registrations.remove(&node_id);
                }
            }
        }
        self.observers[observer_idx].record_queue.clear();
    }

    /// Return and clear pending records (spec: MutationObserver.takeRecords).
    pub fn take_records(&mut self, observer_idx: usize) -> Vec<MutationRecordData> {
        if observer_idx >= self.observers.len() {
            return Vec::new();
        }
        std::mem::take(&mut self.observers[observer_idx].record_queue)
    }
}

// ─── Public notification functions ───────────────────────────────────────────
//
// Called from V8 binding callbacks AFTER the DOM mutation has been performed.
// Each captures the "interested observers" per §4.3.1 then enqueues records.

/// Notify observers of a childList mutation.
pub fn notify_child_list(
    scope: &mut v8::HandleScope,
    parent: NodeId,
    added: &[NodeId],
    removed: &[NodeId],
    prev_sibling: Option<NodeId>,
    next_sibling: Option<NodeId>,
) {
    // Fast path: no observers exist at all
    let has = scope
        .get_slot::<MutationObserverState>()
        .map_or(false, |s| s.has_observers());
    if !has {
        return;
    }

    let arena = arena_ref(scope);
    let ancestors = inclusive_ancestors(arena, parent);

    // Step 2-3: find interested observers
    let interested: Vec<usize> = {
        let state = scope.get_slot::<MutationObserverState>().unwrap();
        let mut result = Vec::new();
        for &ancestor in &ancestors {
            if let Some(regs) = state.registrations.get(&ancestor) {
                for reg in regs {
                    if !reg.options.child_list {
                        continue;
                    }
                    if ancestor != parent && !reg.options.subtree {
                        continue;
                    }
                    if !result.contains(&reg.observer_idx) {
                        result.push(reg.observer_idx);
                    }
                }
            }
        }
        result
    };

    if interested.is_empty() {
        return;
    }

    let record = MutationRecordData {
        mutation_type: "childList",
        target: parent,
        added_nodes: added.to_vec(),
        removed_nodes: removed.to_vec(),
        previous_sibling: prev_sibling,
        next_sibling,
        attribute_name: None,
        attribute_namespace: None,
        old_value: None,
    };

    {
        let state = scope.get_slot_mut::<MutationObserverState>().unwrap();
        for obs_idx in interested {
            state.observers[obs_idx].record_queue.push(record.clone());
        }
    }

    maybe_enqueue_microtask(scope);
}

/// Notify observers of an attribute mutation.
pub fn notify_attribute(
    scope: &mut v8::HandleScope,
    target: NodeId,
    attr_name: &str,
    old_value: Option<&str>,
) {
    let has = scope
        .get_slot::<MutationObserverState>()
        .map_or(false, |s| s.has_observers());
    if !has {
        return;
    }

    let arena = arena_ref(scope);
    let ancestors = inclusive_ancestors(arena, target);

    // Collect interested observers with mapped old values
    let interested: Vec<(usize, Option<String>)> = {
        let state = scope.get_slot::<MutationObserverState>().unwrap();
        let mut result: Vec<(usize, Option<String>)> = Vec::new();
        for &ancestor in &ancestors {
            if let Some(regs) = state.registrations.get(&ancestor) {
                for reg in regs {
                    if !reg.options.attributes {
                        continue;
                    }
                    if ancestor != target && !reg.options.subtree {
                        continue;
                    }
                    // attributeFilter check (spec step 3.2.3)
                    if !reg.options.attribute_filter.is_empty()
                        && !reg.options
                            .attribute_filter
                            .iter()
                            .any(|f| f == attr_name)
                    {
                        continue;
                    }
                    // Per spec: if observer already interested, only update old value
                    if let Some(entry) =
                        result.iter_mut().find(|(idx, _)| *idx == reg.observer_idx)
                    {
                        if reg.options.attribute_old_value {
                            entry.1 = old_value.map(|s| s.to_string());
                        }
                        continue;
                    }
                    let mapped = if reg.options.attribute_old_value {
                        old_value.map(|s| s.to_string())
                    } else {
                        None
                    };
                    result.push((reg.observer_idx, mapped));
                }
            }
        }
        result
    };

    if interested.is_empty() {
        return;
    }

    {
        let state = scope.get_slot_mut::<MutationObserverState>().unwrap();
        for (obs_idx, mapped_old_value) in interested {
            state.observers[obs_idx]
                .record_queue
                .push(MutationRecordData {
                    mutation_type: "attributes",
                    target,
                    added_nodes: Vec::new(),
                    removed_nodes: Vec::new(),
                    previous_sibling: None,
                    next_sibling: None,
                    attribute_name: Some(attr_name.to_string()),
                    attribute_namespace: None,
                    old_value: mapped_old_value,
                });
        }
    }

    maybe_enqueue_microtask(scope);
}

/// Notify observers of a characterData mutation.
pub fn notify_character_data(
    scope: &mut v8::HandleScope,
    target: NodeId,
    old_value: &str,
) {
    let has = scope
        .get_slot::<MutationObserverState>()
        .map_or(false, |s| s.has_observers());
    if !has {
        return;
    }

    let arena = arena_ref(scope);
    let ancestors = inclusive_ancestors(arena, target);

    let interested: Vec<(usize, Option<String>)> = {
        let state = scope.get_slot::<MutationObserverState>().unwrap();
        let mut result: Vec<(usize, Option<String>)> = Vec::new();
        for &ancestor in &ancestors {
            if let Some(regs) = state.registrations.get(&ancestor) {
                for reg in regs {
                    if !reg.options.character_data {
                        continue;
                    }
                    if ancestor != target && !reg.options.subtree {
                        continue;
                    }
                    if let Some(entry) =
                        result.iter_mut().find(|(idx, _)| *idx == reg.observer_idx)
                    {
                        if reg.options.character_data_old_value {
                            entry.1 = Some(old_value.to_string());
                        }
                        continue;
                    }
                    let mapped = if reg.options.character_data_old_value {
                        Some(old_value.to_string())
                    } else {
                        None
                    };
                    result.push((reg.observer_idx, mapped));
                }
            }
        }
        result
    };

    if interested.is_empty() {
        return;
    }

    {
        let state = scope.get_slot_mut::<MutationObserverState>().unwrap();
        for (obs_idx, mapped) in interested {
            state.observers[obs_idx]
                .record_queue
                .push(MutationRecordData {
                    mutation_type: "characterData",
                    target,
                    added_nodes: Vec::new(),
                    removed_nodes: Vec::new(),
                    previous_sibling: None,
                    next_sibling: None,
                    attribute_name: None,
                    attribute_namespace: None,
                    old_value: mapped,
                });
        }
    }

    maybe_enqueue_microtask(scope);
}

// ─── Internal helpers ────────────────────────────────────────────────────────

/// Collect inclusive ancestors: [node, parent, grandparent, ..., document].
fn inclusive_ancestors(arena: &crate::dom::Arena, node: NodeId) -> Vec<NodeId> {
    let mut result = vec![node];
    let mut current = arena.nodes[node].parent;
    while let Some(id) = current {
        result.push(id);
        current = arena.nodes[id].parent;
    }
    result
}

/// Ensure a single microtask is queued to dispatch pending records.
fn maybe_enqueue_microtask(scope: &mut v8::HandleScope) {
    let already_queued = scope
        .get_slot::<MutationObserverState>()
        .map_or(true, |s| s.microtask_queued);
    if already_queued {
        return;
    }

    {
        let state = scope.get_slot_mut::<MutationObserverState>().unwrap();
        state.microtask_queued = true;
    }

    let dispatch = v8::Function::new(scope, dispatch_mutation_observers).unwrap();
    scope.enqueue_microtask(dispatch);
}

/// V8 microtask callback: fire all pending observer callbacks.
/// Per spec §4.3.1 step 5: "Queue a mutation observer microtask."
fn dispatch_mutation_observers(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    // Extract pending data, releasing the borrow before calling into JS
    let pending: Vec<(
        v8::Global<v8::Function>,
        v8::Global<v8::Object>,
        Vec<MutationRecordData>,
    )> = {
        let state = scope.get_slot_mut::<MutationObserverState>().unwrap();
        state.microtask_queued = false;
        state
            .observers
            .iter_mut()
            .filter(|e| !e.record_queue.is_empty())
            .map(|e| {
                (
                    e.callback.clone(),
                    e.js_object.clone(),
                    std::mem::take(&mut e.record_queue),
                )
            })
            .collect()
    };

    for (callback_global, observer_global, records) in pending {
        // Build V8 Array of MutationRecord objects
        let arr = v8::Array::new(scope, records.len() as i32);
        for (i, rec) in records.iter().enumerate() {
            let obj = build_record_object(scope, rec);
            arr.set_index(scope, i as u32, obj.into());
        }

        let callback = v8::Local::new(scope, &callback_global);
        let observer = v8::Local::new(scope, &observer_global);
        let undefined = v8::undefined(scope);
        // Per spec: callback(records, observer)
        let _ = callback.call(scope, undefined.into(), &[arr.into(), observer.into()]);
    }
}

/// Build a plain JS object representing a MutationRecord.
fn build_record_object<'s>(
    scope: &mut v8::HandleScope<'s>,
    record: &MutationRecordData,
) -> v8::Local<'s, v8::Object> {
    let obj = v8::Object::new(scope);

    // type
    let k = v8::String::new(scope, "type").unwrap();
    let v = v8::String::new(scope, record.mutation_type).unwrap();
    obj.set(scope, k.into(), v.into());

    // target
    let k = v8::String::new(scope, "target").unwrap();
    let v = wrap_node(scope, record.target);
    obj.set(scope, k.into(), v.into());

    // addedNodes (NodeList-like array)
    let k = v8::String::new(scope, "addedNodes").unwrap();
    let arr = v8::Array::new(scope, record.added_nodes.len() as i32);
    for (i, &nid) in record.added_nodes.iter().enumerate() {
        let w = wrap_node(scope, nid);
        arr.set_index(scope, i as u32, w.into());
    }
    obj.set(scope, k.into(), arr.into());

    // removedNodes (NodeList-like array)
    let k = v8::String::new(scope, "removedNodes").unwrap();
    let arr = v8::Array::new(scope, record.removed_nodes.len() as i32);
    for (i, &nid) in record.removed_nodes.iter().enumerate() {
        let w = wrap_node(scope, nid);
        arr.set_index(scope, i as u32, w.into());
    }
    obj.set(scope, k.into(), arr.into());

    // previousSibling
    {
        let k = v8::String::new(scope, "previousSibling").unwrap();
        let v: v8::Local<v8::Value> = match record.previous_sibling {
            Some(nid) => wrap_node(scope, nid).into(),
            None => v8::null(scope).into(),
        };
        obj.set(scope, k.into(), v);
    }

    // nextSibling
    {
        let k = v8::String::new(scope, "nextSibling").unwrap();
        let v: v8::Local<v8::Value> = match record.next_sibling {
            Some(nid) => wrap_node(scope, nid).into(),
            None => v8::null(scope).into(),
        };
        obj.set(scope, k.into(), v);
    }

    // attributeName
    {
        let k = v8::String::new(scope, "attributeName").unwrap();
        let v: v8::Local<v8::Value> = match &record.attribute_name {
            Some(n) => v8::String::new(scope, n).unwrap().into(),
            None => v8::null(scope).into(),
        };
        obj.set(scope, k.into(), v);
    }

    // attributeNamespace
    {
        let k = v8::String::new(scope, "attributeNamespace").unwrap();
        let v: v8::Local<v8::Value> = match &record.attribute_namespace {
            Some(n) => v8::String::new(scope, n).unwrap().into(),
            None => v8::null(scope).into(),
        };
        obj.set(scope, k.into(), v);
    }

    // oldValue
    {
        let k = v8::String::new(scope, "oldValue").unwrap();
        let v: v8::Local<v8::Value> = match &record.old_value {
            Some(val) => v8::String::new(scope, val).unwrap().into(),
            None => v8::null(scope).into(),
        };
        obj.set(scope, k.into(), v);
    }

    obj
}

/// Build records as a V8 Array — used by takeRecords.
pub fn build_records_array<'s>(
    scope: &mut v8::HandleScope<'s>,
    observer_idx: usize,
) -> v8::Local<'s, v8::Array> {
    let records = {
        let state = scope.get_slot_mut::<MutationObserverState>().unwrap();
        state.take_records(observer_idx)
    };
    let arr = v8::Array::new(scope, records.len() as i32);
    for (i, rec) in records.iter().enumerate() {
        let obj = build_record_object(scope, rec);
        arr.set_index(scope, i as u32, obj.into());
    }
    arr
}
