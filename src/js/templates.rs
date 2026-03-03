/// V8 ObjectTemplate definitions for DOM types.
///
/// Creates the template hierarchy (Node → Document/Element/Text/Comment),
/// and provides wrap_node/unwrap_node_id for bridging Arena ↔ V8.

use std::collections::HashMap;

use crate::dom::arena::{Arena, NodeId};
use crate::dom::node::NodeData;
use crate::js::runtime::ArenaPtr;

/// Cache of NodeId → JS wrapper object for identity semantics (===).
pub struct WrapperCache {
    pub map: HashMap<NodeId, v8::Global<v8::Object>>,
}

/// Cache of NodeId → live childNodes Proxy for identity semantics.
pub struct ChildNodesCache {
    pub map: HashMap<NodeId, v8::Global<v8::Object>>,
}

/// Cached ObjectTemplates for each DOM type, stored in isolate slot.
pub struct DomTemplates {
    pub document_template: v8::Global<v8::ObjectTemplate>,
    pub element_template: v8::Global<v8::ObjectTemplate>,
    pub text_template: v8::Global<v8::ObjectTemplate>,
    pub comment_template: v8::Global<v8::ObjectTemplate>,
}

/// Create all DOM type templates. Called once per isolate, before context creation.
pub fn create_dom_templates(scope: &mut v8::HandleScope<()>) -> DomTemplates {
    // --- Node base template ---
    let node_ft = v8::FunctionTemplate::new(scope, throwing_constructor);
    let node_class = v8::String::new(scope, "Node").unwrap();
    node_ft.set_class_name(node_class);
    let node_proto = node_ft.prototype_template(scope);
    let node_inst = node_ft.instance_template(scope);
    node_inst.set_internal_field_count(1);
    super::bindings::node::install(scope, &node_proto);

    // --- Document extends Node ---
    let doc_ft = v8::FunctionTemplate::new(scope, throwing_constructor);
    doc_ft.inherit(node_ft);
    let doc_class = v8::String::new(scope, "Document").unwrap();
    doc_ft.set_class_name(doc_class);
    let doc_proto = doc_ft.prototype_template(scope);
    let doc_inst = doc_ft.instance_template(scope);
    doc_inst.set_internal_field_count(1);
    super::bindings::document::install(scope, &doc_proto);

    // --- Element extends Node ---
    let elem_ft = v8::FunctionTemplate::new(scope, throwing_constructor);
    elem_ft.inherit(node_ft);
    let elem_class = v8::String::new(scope, "Element").unwrap();
    elem_ft.set_class_name(elem_class);
    let elem_proto = elem_ft.prototype_template(scope);
    let elem_inst = elem_ft.instance_template(scope);
    elem_inst.set_internal_field_count(1);
    super::bindings::element::install(scope, &elem_proto);

    // --- Text extends Node ---
    let text_ft = v8::FunctionTemplate::new(scope, throwing_constructor);
    text_ft.inherit(node_ft);
    let text_class = v8::String::new(scope, "Text").unwrap();
    text_ft.set_class_name(text_class);
    let text_proto = text_ft.prototype_template(scope);
    let text_inst = text_ft.instance_template(scope);
    text_inst.set_internal_field_count(1);
    super::bindings::text::install(scope, &text_proto);

    // --- Comment extends Node ---
    let comment_ft = v8::FunctionTemplate::new(scope, throwing_constructor);
    comment_ft.inherit(node_ft);
    let comment_class = v8::String::new(scope, "Comment").unwrap();
    comment_ft.set_class_name(comment_class);
    let comment_proto = comment_ft.prototype_template(scope);
    let comment_inst = comment_ft.instance_template(scope);
    comment_inst.set_internal_field_count(1);
    super::bindings::comment::install(scope, &comment_proto);

    DomTemplates {
        document_template: v8::Global::new(scope, doc_inst),
        element_template: v8::Global::new(scope, elem_inst),
        text_template: v8::Global::new(scope, text_inst),
        comment_template: v8::Global::new(scope, comment_inst),
    }
}

/// Constructor that always throws — DOM objects aren't user-constructible.
fn throwing_constructor(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let msg = v8::String::new(scope, "Illegal constructor").unwrap();
    let exception = v8::Exception::type_error(scope, msg);
    scope.throw_exception(exception);
}

/// Create the global object template (empty — globals installed after context creation).
pub fn create_global_template<'s>(scope: &mut v8::HandleScope<'s, ()>) -> v8::Local<'s, v8::ObjectTemplate> {
    v8::ObjectTemplate::new(scope)
}

/// Create or retrieve a JS wrapper for a DOM node.
///
/// Returns cached wrapper if available (identity semantics: same node = same object).
pub fn wrap_node<'s>(
    scope: &mut v8::HandleScope<'s>,
    node_id: NodeId,
) -> v8::Local<'s, v8::Object> {
    // Check cache first — clone the Global handle out to avoid holding the borrow
    let cached = scope
        .get_slot::<WrapperCache>()
        .and_then(|cache| cache.map.get(&node_id).cloned());
    if let Some(global) = cached {
        return v8::Local::new(scope, &global);
    }

    // Determine which template to use based on node type.
    // We need to figure out the node kind and clone the right Global<ObjectTemplate>
    // before we can use `scope` mutably.
    let arena_ptr = scope.get_slot::<ArenaPtr>().unwrap().0;
    let arena = unsafe { &*arena_ptr };
    let node_kind = match &arena.nodes[node_id].data {
        NodeData::Document => NodeKind::Document,
        NodeData::DocumentFragment => NodeKind::Document, // Uses Node template like Document
        NodeData::Element(_) => NodeKind::Element,
        NodeData::Text(_) => NodeKind::Text,
        NodeData::Comment(_) => NodeKind::Comment,
        NodeData::Doctype { .. } => NodeKind::Comment,
    };

    let templates = scope.get_slot::<DomTemplates>().unwrap();
    let template_global = match node_kind {
        NodeKind::Document => templates.document_template.clone(),
        NodeKind::Element => templates.element_template.clone(),
        NodeKind::Text => templates.text_template.clone(),
        NodeKind::Comment => templates.comment_template.clone(),
    };

    let template = v8::Local::new(scope, &template_global);
    let obj = template.new_instance(scope).unwrap();

    // Store NodeId in internal field 0 via External
    let boxed = Box::new(node_id);
    let external =
        v8::External::new(scope, Box::into_raw(boxed) as *mut std::ffi::c_void);
    obj.set_internal_field(0, external.into());

    // Cache the wrapper
    let global = v8::Global::new(scope, obj);
    let cache = scope.get_slot_mut::<WrapperCache>().unwrap();
    cache.map.insert(node_id, global);

    obj
}

#[derive(Clone, Copy)]
enum NodeKind {
    Document,
    Element,
    Text,
    Comment,
}

/// Extract NodeId from a JS wrapper object's internal field.
pub fn unwrap_node_id(
    scope: &mut v8::HandleScope,
    obj: v8::Local<v8::Object>,
) -> Option<NodeId> {
    let data = obj.get_internal_field(scope, 0)?;
    let value: v8::Local<v8::Value> = data.try_into().ok()?;
    let ext = v8::Local::<v8::External>::try_from(value).ok()?;
    let ptr = ext.value() as *const NodeId;
    if ptr.is_null() {
        return None;
    }
    Some(unsafe { *ptr })
}

/// Get a shared Arena reference from the isolate slot.
///
/// # Safety
/// Safe because: single-threaded, Arena outlives Isolate by stack construction.
pub fn arena_ref<'a>(scope: &mut v8::HandleScope) -> &'a Arena {
    let ptr = scope.get_slot::<ArenaPtr>().unwrap().0;
    unsafe { &*ptr }
}

/// Get a mutable Arena reference from the isolate slot.
///
/// # Safety
/// Safe because: single-threaded, Arena outlives Isolate by stack construction,
/// and V8 callbacks never nest Arena mutations.
pub fn arena_mut<'a>(scope: &mut v8::HandleScope) -> &'a mut Arena {
    let ptr = scope.get_slot::<ArenaPtr>().unwrap().0;
    unsafe { &mut *ptr }
}
