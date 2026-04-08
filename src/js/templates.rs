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

/// Cached ObjectTemplates and FunctionTemplates for the DOM type hierarchy.
///
/// Hierarchy:
///   Node
///     ├── Document
///     ├── DocumentFragment
///     ├── DocumentType
///     ├── CharacterData
///     │     ├── Text
///     │     └── Comment
///     └── Element
///           ├── HTMLElement
///           │     └── HTMLMediaElement
///           └── SVGElement
pub struct DomTemplates {
    // ObjectTemplates for wrap_node instantiation
    pub document_template: v8::Global<v8::ObjectTemplate>,
    pub doc_fragment_template: v8::Global<v8::ObjectTemplate>,
    pub doctype_template: v8::Global<v8::ObjectTemplate>,
    pub element_template: v8::Global<v8::ObjectTemplate>,
    pub html_element_template: v8::Global<v8::ObjectTemplate>,
    pub html_media_template: v8::Global<v8::ObjectTemplate>,
    pub text_template: v8::Global<v8::ObjectTemplate>,
    pub comment_template: v8::Global<v8::ObjectTemplate>,

    // FunctionTemplates for exposing constructors with real prototypes
    pub node_function: v8::Global<v8::FunctionTemplate>,
    pub document_function: v8::Global<v8::FunctionTemplate>,
    pub doc_fragment_function: v8::Global<v8::FunctionTemplate>,
    pub doctype_function: v8::Global<v8::FunctionTemplate>,
    pub characterdata_function: v8::Global<v8::FunctionTemplate>,
    pub element_function: v8::Global<v8::FunctionTemplate>,
    pub html_element_function: v8::Global<v8::FunctionTemplate>,
    pub html_media_function: v8::Global<v8::FunctionTemplate>,
    pub svg_element_function: v8::Global<v8::FunctionTemplate>,
    pub text_function: v8::Global<v8::FunctionTemplate>,
    pub comment_function: v8::Global<v8::FunctionTemplate>,
}

/// Create all DOM type templates. Called once per isolate, before context creation.
///
/// Builds the spec-correct inheritance hierarchy via FunctionTemplate::inherit().
pub fn create_dom_templates(scope: &mut v8::HandleScope<()>) -> DomTemplates {
    // ─── Node (base) ────────────────────────────────────────────────────
    let node_ft = v8::FunctionTemplate::new(scope, throwing_constructor);
    let node_class = v8::String::new(scope, "Node").unwrap();
    node_ft.set_class_name(node_class);
    let node_proto = node_ft.prototype_template(scope);
    let node_inst = node_ft.instance_template(scope);
    node_inst.set_internal_field_count(1);
    super::bindings::node::install(scope, &node_proto);

    // ─── Document extends Node ──────────────────────────────────────────
    let doc_ft = v8::FunctionTemplate::new(scope, throwing_constructor);
    doc_ft.inherit(node_ft);
    let doc_class = v8::String::new(scope, "Document").unwrap();
    doc_ft.set_class_name(doc_class);
    let doc_proto = doc_ft.prototype_template(scope);
    let doc_inst = doc_ft.instance_template(scope);
    doc_inst.set_internal_field_count(1);
    super::bindings::document::install(scope, &doc_proto);

    // ─── DocumentFragment extends Node ──────────────────────────────────
    let docfrag_ft = v8::FunctionTemplate::new(scope, throwing_constructor);
    docfrag_ft.inherit(node_ft);
    let docfrag_class = v8::String::new(scope, "DocumentFragment").unwrap();
    docfrag_ft.set_class_name(docfrag_class);
    let docfrag_proto = docfrag_ft.prototype_template(scope);
    let docfrag_inst = docfrag_ft.instance_template(scope);
    docfrag_inst.set_internal_field_count(1);
    // ParentNode mixin: querySelector, querySelectorAll, children, etc.
    // These work on any node type that has children in the arena.
    super::bindings::element::install_parent_node_mixin(scope, &docfrag_proto);

    // ─── DocumentType extends Node ──────────────────────────────────────
    let doctype_ft = v8::FunctionTemplate::new(scope, throwing_constructor);
    doctype_ft.inherit(node_ft);
    let doctype_class = v8::String::new(scope, "DocumentType").unwrap();
    doctype_ft.set_class_name(doctype_class);
    let doctype_proto = doctype_ft.prototype_template(scope);
    let doctype_inst = doctype_ft.instance_template(scope);
    doctype_inst.set_internal_field_count(1);
    super::bindings::doctype::install(scope, &doctype_proto);

    // ─── CharacterData extends Node ─────────────────────────────────────
    let chardata_ft = v8::FunctionTemplate::new(scope, throwing_constructor);
    chardata_ft.inherit(node_ft);
    let chardata_class = v8::String::new(scope, "CharacterData").unwrap();
    chardata_ft.set_class_name(chardata_class);
    let chardata_proto = chardata_ft.prototype_template(scope);
    let chardata_inst = chardata_ft.instance_template(scope);
    chardata_inst.set_internal_field_count(1);
    super::bindings::characterdata::install(scope, &chardata_proto);

    // ─── Text extends CharacterData ─────────────────────────────────────
    let text_ft = v8::FunctionTemplate::new(scope, throwing_constructor);
    text_ft.inherit(chardata_ft);  // CharacterData, NOT Node
    let text_class = v8::String::new(scope, "Text").unwrap();
    text_ft.set_class_name(text_class);
    let text_proto = text_ft.prototype_template(scope);
    let text_inst = text_ft.instance_template(scope);
    text_inst.set_internal_field_count(1);
    super::bindings::text::install(scope, &text_proto);

    // ─── Comment extends CharacterData ──────────────────────────────────
    let comment_ft = v8::FunctionTemplate::new(scope, throwing_constructor);
    comment_ft.inherit(chardata_ft);  // CharacterData, NOT Node
    let comment_class = v8::String::new(scope, "Comment").unwrap();
    comment_ft.set_class_name(comment_class);
    let comment_proto = comment_ft.prototype_template(scope);
    let comment_inst = comment_ft.instance_template(scope);
    comment_inst.set_internal_field_count(1);
    super::bindings::comment::install(scope, &comment_proto);

    // ─── Element extends Node ───────────────────────────────────────────
    let elem_ft = v8::FunctionTemplate::new(scope, throwing_constructor);
    elem_ft.inherit(node_ft);
    let elem_class = v8::String::new(scope, "Element").unwrap();
    elem_ft.set_class_name(elem_class);
    let elem_proto = elem_ft.prototype_template(scope);
    let elem_inst = elem_ft.instance_template(scope);
    elem_inst.set_internal_field_count(1);
    super::bindings::element::install(scope, &elem_proto);

    // ─── HTMLElement extends Element ────────────────────────────────────
    let html_elem_ft = v8::FunctionTemplate::new(scope, throwing_constructor);
    html_elem_ft.inherit(elem_ft);
    let html_elem_class = v8::String::new(scope, "HTMLElement").unwrap();
    html_elem_ft.set_class_name(html_elem_class);
    let html_elem_inst = html_elem_ft.instance_template(scope);
    html_elem_inst.set_internal_field_count(1);
    // HTMLElement-specific methods are currently on Element.prototype
    // (title, lang, dir, etc.) — they inherit correctly through the chain.

    // ─── HTMLMediaElement extends HTMLElement ────────────────────────────
    let media_ft = v8::FunctionTemplate::new(scope, throwing_constructor);
    media_ft.inherit(html_elem_ft);
    let media_class = v8::String::new(scope, "HTMLMediaElement").unwrap();
    media_ft.set_class_name(media_class);
    let media_proto = media_ft.prototype_template(scope);
    let media_inst = media_ft.instance_template(scope);
    media_inst.set_internal_field_count(1);
    super::bindings::htmlmediaelement::install(scope, &media_proto);

    // ─── SVGElement extends Element ─────────────────────────────────────
    let svg_ft = v8::FunctionTemplate::new(scope, throwing_constructor);
    svg_ft.inherit(elem_ft);
    let svg_class = v8::String::new(scope, "SVGElement").unwrap();
    svg_ft.set_class_name(svg_class);
    let svg_inst = svg_ft.instance_template(scope);
    svg_inst.set_internal_field_count(1);

    DomTemplates {
        // ObjectTemplates for wrap_node
        document_template: v8::Global::new(scope, doc_inst),
        doc_fragment_template: v8::Global::new(scope, docfrag_inst),
        doctype_template: v8::Global::new(scope, doctype_inst),
        element_template: v8::Global::new(scope, elem_inst),
        html_element_template: v8::Global::new(scope, html_elem_inst),
        html_media_template: v8::Global::new(scope, media_inst),
        text_template: v8::Global::new(scope, text_inst),
        comment_template: v8::Global::new(scope, comment_inst),

        // FunctionTemplates for constructor prototypes
        node_function: v8::Global::new(scope, node_ft),
        document_function: v8::Global::new(scope, doc_ft),
        doc_fragment_function: v8::Global::new(scope, docfrag_ft),
        doctype_function: v8::Global::new(scope, doctype_ft),
        characterdata_function: v8::Global::new(scope, chardata_ft),
        element_function: v8::Global::new(scope, elem_ft),
        html_element_function: v8::Global::new(scope, html_elem_ft),
        html_media_function: v8::Global::new(scope, media_ft),
        svg_element_function: v8::Global::new(scope, svg_ft),
        text_function: v8::Global::new(scope, text_ft),
        comment_function: v8::Global::new(scope, comment_ft),
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
        NodeData::DocumentFragment => NodeKind::DocumentFragment,
        NodeData::Element(data) => {
            let tag = &*data.name.local;
            if tag == "video" || tag == "audio" {
                NodeKind::HTMLMediaElement
            } else {
                // All HTML elements use HTMLElement template
                // (SVG detection could use namespace check but not needed for SSR)
                NodeKind::HTMLElement
            }
        }
        NodeData::Text(_) => NodeKind::Text,
        NodeData::Comment(_) => NodeKind::Comment,
        NodeData::Doctype { .. } => NodeKind::Doctype,
    };

    let templates = scope.get_slot::<DomTemplates>().unwrap();
    let template_global = match node_kind {
        NodeKind::Document => templates.document_template.clone(),
        NodeKind::DocumentFragment => templates.doc_fragment_template.clone(),
        NodeKind::Doctype => templates.doctype_template.clone(),
        NodeKind::HTMLElement => templates.html_element_template.clone(),
        NodeKind::HTMLMediaElement => templates.html_media_template.clone(),
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

    // Set element-specific IDL properties
    if let NodeData::Element(data) = &arena.nodes[node_id].data {
        let tag = &*data.name.local;
        if tag == "canvas" {
            let k = v8::String::new(scope, "width").unwrap();
            let v = v8::Integer::new(scope, 300);
            obj.set(scope, k.into(), v.into());
            let k = v8::String::new(scope, "height").unwrap();
            let v = v8::Integer::new(scope, 150);
            obj.set(scope, k.into(), v.into());
        }

        // HTMLMediaElement methods are now on the prototype (via htmlmediaelement::install)
        // No per-instance install needed for <video>/<audio>.

        // HTMLFormElement properties for <form>
        if tag == "form" {
            install_form_element_props(scope, obj);
        }

        // HTMLSelectElement properties for <select>
        if tag == "select" {
            install_select_element_props(scope, obj);
        }

        // HTMLSlotElement properties for <slot>
        if tag == "slot" {
            install_slot_element_props(scope, obj, node_id);
        }

        // HTMLTemplateElement: .content property returning the template's DocumentFragment
        if tag == "template" {
            install_template_element_props(scope, obj, node_id);
        }
    }

    // Cache the wrapper
    let global = v8::Global::new(scope, obj);
    let cache = scope.get_slot_mut::<WrapperCache>().unwrap();
    cache.map.insert(node_id, global);

    obj
}

#[derive(Clone, Copy)]
enum NodeKind {
    Document,
    DocumentFragment,
    Doctype,
    HTMLElement,
    HTMLMediaElement,
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

// ─── Element-specific property installers ───────────────────────────────────

/// Install HTMLMediaElement properties on a video/audio wrapper.
fn install_media_element_props(scope: &mut v8::HandleScope, obj: v8::Local<v8::Object>) {
    // play() — returns resolved Promise
    let play_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        log::trace!("HTMLMediaElement.play() called (SSR stub)");
        let resolver = v8::PromiseResolver::new(scope).unwrap();
        let undef = v8::undefined(scope);
        resolver.resolve(scope, undef.into());
        rv.set(resolver.get_promise(scope).into());
    }).unwrap();
    let k = v8::String::new(scope, "play").unwrap();
    obj.set(scope, k.into(), play_fn.into());

    // pause(), load() — no-ops
    let noop = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    for name in &["pause", "load"] {
        let k = v8::String::new(scope, name).unwrap();
        obj.set(scope, k.into(), noop.into());
    }

    // canPlayType() — returns ""
    let cpt = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let empty = v8::String::new(scope, "").unwrap();
        rv.set(empty.into());
    }).unwrap();
    let k = v8::String::new(scope, "canPlayType").unwrap();
    obj.set(scope, k.into(), cpt.into());

    // Boolean properties
    let t = v8::Boolean::new(scope, true);
    let f = v8::Boolean::new(scope, false);
    let k = v8::String::new(scope, "paused").unwrap();
    obj.set(scope, k.into(), t.into());
    let k = v8::String::new(scope, "ended").unwrap();
    obj.set(scope, k.into(), f.into());
    let k = v8::String::new(scope, "muted").unwrap();
    obj.set(scope, k.into(), f.into());
    let k = v8::String::new(scope, "autoplay").unwrap();
    obj.set(scope, k.into(), f.into());
    let k = v8::String::new(scope, "loop").unwrap();
    obj.set(scope, k.into(), f.into());
    let k = v8::String::new(scope, "controls").unwrap();
    obj.set(scope, k.into(), f.into());

    // Number properties
    let zero = v8::Number::new(scope, 0.0);
    let k = v8::String::new(scope, "currentTime").unwrap();
    obj.set(scope, k.into(), zero.into());
    let nan = v8::Number::new(scope, f64::NAN);
    let k = v8::String::new(scope, "duration").unwrap();
    obj.set(scope, k.into(), nan.into());
    let one = v8::Number::new(scope, 1.0);
    let k = v8::String::new(scope, "volume").unwrap();
    obj.set(scope, k.into(), one.into());
    let one_f = v8::Number::new(scope, 1.0);
    let k = v8::String::new(scope, "playbackRate").unwrap();
    obj.set(scope, k.into(), one_f.into());
    let zero_i = v8::Integer::new(scope, 0);
    let k = v8::String::new(scope, "readyState").unwrap();
    obj.set(scope, k.into(), zero_i.into());
    let zero_i2 = v8::Integer::new(scope, 0);
    let k = v8::String::new(scope, "networkState").unwrap();
    obj.set(scope, k.into(), zero_i2.into());

    // String properties
    let empty = v8::String::new(scope, "").unwrap();
    let k = v8::String::new(scope, "currentSrc").unwrap();
    obj.set(scope, k.into(), empty.into());
    let empty2 = v8::String::new(scope, "").unwrap();
    let k = v8::String::new(scope, "preload").unwrap();
    obj.set(scope, k.into(), empty2.into());

    // error = null
    let null = v8::null(scope);
    let k = v8::String::new(scope, "error").unwrap();
    obj.set(scope, k.into(), null.into());

    // buffered / seekable / played — TimeRanges stub {length: 0}
    for name in &["buffered", "seekable", "played"] {
        let tr = v8::Object::new(scope);
        let zero_v = v8::Integer::new(scope, 0);
        let k = v8::String::new(scope, "length").unwrap();
        tr.set(scope, k.into(), zero_v.into());
        let start_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {
            let msg = v8::String::new(scope, "IndexSizeError").unwrap();
            let exc = v8::Exception::range_error(scope, msg);
            scope.throw_exception(exc);
        }).unwrap();
        let k = v8::String::new(scope, "start").unwrap();
        tr.set(scope, k.into(), start_fn.into());
        let end_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {
            let msg = v8::String::new(scope, "IndexSizeError").unwrap();
            let exc = v8::Exception::range_error(scope, msg);
            scope.throw_exception(exc);
        }).unwrap();
        let k = v8::String::new(scope, "end").unwrap();
        tr.set(scope, k.into(), end_fn.into());
        let k = v8::String::new(scope, name).unwrap();
        obj.set(scope, k.into(), tr.into());
    }

    // textTracks — empty array-like
    let tracks = v8::Array::new(scope, 0);
    let k = v8::String::new(scope, "textTracks").unwrap();
    obj.set(scope, k.into(), tracks.into());

    // addEventListener/removeEventListener no-ops (for media events)
    let noop2 = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    for name in &["addEventListener", "removeEventListener"] {
        let k = v8::String::new(scope, name).unwrap();
        // Only set if not already defined (from Node prototype)
        if obj.get(scope, k.into()).map(|v| v.is_undefined()).unwrap_or(true) {
            obj.set(scope, k.into(), noop2.into());
        }
    }

    log::trace!("Installed HTMLMediaElement properties on wrapper");
}

/// Install HTMLFormElement properties on a form wrapper.
fn install_form_element_props(scope: &mut v8::HandleScope, obj: v8::Local<v8::Object>) {
    // elements — empty array-like with length
    let elements = v8::Array::new(scope, 0);
    let k = v8::String::new(scope, "elements").unwrap();
    obj.set(scope, k.into(), elements.into());

    // submit(), reset(), requestSubmit() — no-ops
    let noop = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {
        log::trace!("HTMLFormElement method called (no-op in SSR)");
    }).unwrap();
    for name in &["submit", "reset", "requestSubmit"] {
        let k = v8::String::new(scope, name).unwrap();
        obj.set(scope, k.into(), noop.into());
    }

    // length — 0
    let k = v8::String::new(scope, "length").unwrap();
    let v = v8::Integer::new(scope, 0);
    obj.set(scope, k.into(), v.into());

    log::trace!("Installed HTMLFormElement properties on wrapper");
}

/// Install HTMLSelectElement properties on a select wrapper.
fn install_select_element_props(scope: &mut v8::HandleScope, obj: v8::Local<v8::Object>) {
    // selectedIndex — -1
    let k = v8::String::new(scope, "selectedIndex").unwrap();
    let v = v8::Integer::new(scope, -1);
    obj.set(scope, k.into(), v.into());

    // options — empty array-like
    let options = v8::Array::new(scope, 0);
    let k = v8::String::new(scope, "options").unwrap();
    obj.set(scope, k.into(), options.into());

    // selectedOptions — empty array-like
    let selected = v8::Array::new(scope, 0);
    let k = v8::String::new(scope, "selectedOptions").unwrap();
    obj.set(scope, k.into(), selected.into());

    // length — 0
    let k = v8::String::new(scope, "length").unwrap();
    let v = v8::Integer::new(scope, 0);
    obj.set(scope, k.into(), v.into());

    log::trace!("Installed HTMLSelectElement properties on wrapper");
}

/// Install HTMLSlotElement properties (assignedNodes, assignedElements).
fn install_slot_element_props(scope: &mut v8::HandleScope, obj: v8::Local<v8::Object>, _slot_id: NodeId) {
    // assignedNodes() — returns light DOM children of the shadow host that match this slot
    let assigned_nodes_fn = v8::Function::new(scope, slot_assigned_nodes).unwrap();
    let k = v8::String::new(scope, "assignedNodes").unwrap();
    obj.set(scope, k.into(), assigned_nodes_fn.into());

    // assignedElements() — same but only Element children
    let assigned_elements_fn = v8::Function::new(scope, slot_assigned_elements).unwrap();
    let k = v8::String::new(scope, "assignedElements").unwrap();
    obj.set(scope, k.into(), assigned_elements_fn.into());

    log::trace!("Installed HTMLSlotElement properties on wrapper");
}

/// Get the slot name from a <slot> element's "name" attribute.
fn get_slot_name(arena: &Arena, slot_id: NodeId) -> String {
    if let NodeData::Element(data) = &arena.nodes[slot_id].data {
        data.get_attribute("name").unwrap_or("").to_string()
    } else {
        String::new()
    }
}

/// Find the shadow host for a slot element by walking up to the shadow root,
/// then finding the host.
fn find_host_for_slot(arena: &Arena, slot_id: NodeId) -> Option<NodeId> {
    // Walk up from slot to find the DocumentFragment (shadow root)
    let mut current = arena.nodes[slot_id].parent;
    while let Some(id) = current {
        if matches!(&arena.nodes[id].data, NodeData::DocumentFragment) {
            // Found the shadow root — find its host
            for (elem_id, node) in arena.nodes.iter() {
                if let NodeData::Element(data) = &node.data {
                    if data.shadow_root == Some(id) {
                        return Some(elem_id);
                    }
                }
            }
            return None;
        }
        current = arena.nodes[id].parent;
    }
    None
}

fn slot_assigned_nodes(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(slot_id) = unwrap_node_id(scope, args.this()) else {
        rv.set(v8::Array::new(scope, 0).into());
        return;
    };
    let arena = arena_ref(scope);
    let slot_name = get_slot_name(arena, slot_id);
    let host_id = find_host_for_slot(arena, slot_id);

    let mut assigned = Vec::new();
    if let Some(host) = host_id {
        // Walk light DOM children of the host
        for child in arena.children(host) {
            let child_slot = match &arena.nodes[child].data {
                NodeData::Element(data) => data.get_attribute("slot").unwrap_or("").to_string(),
                _ => String::new(), // Text/Comment nodes go to default slot
            };
            if child_slot == slot_name {
                assigned.push(child);
            }
        }
    }

    let arr = v8::Array::new(scope, assigned.len() as i32);
    for (i, id) in assigned.iter().enumerate() {
        let wrapped = wrap_node(scope, *id);
        arr.set_index(scope, i as u32, wrapped.into());
    }
    rv.set(arr.into());
}

fn slot_assigned_elements(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(slot_id) = unwrap_node_id(scope, args.this()) else {
        rv.set(v8::Array::new(scope, 0).into());
        return;
    };
    let arena = arena_ref(scope);
    let slot_name = get_slot_name(arena, slot_id);
    let host_id = find_host_for_slot(arena, slot_id);

    let mut assigned = Vec::new();
    if let Some(host) = host_id {
        for child in arena.children(host) {
            // Only include Element nodes
            if let NodeData::Element(data) = &arena.nodes[child].data {
                let child_slot = data.get_attribute("slot").unwrap_or("").to_string();
                if child_slot == slot_name {
                    assigned.push(child);
                }
            }
        }
    }

    let arr = v8::Array::new(scope, assigned.len() as i32);
    for (i, id) in assigned.iter().enumerate() {
        let wrapped = wrap_node(scope, *id);
        arr.set_index(scope, i as u32, wrapped.into());
    }
    rv.set(arr.into());
}

// ─── HTMLTemplateElement: .content + innerHTML redirect ─────────────────────

/// Install template-specific properties: .content (DocumentFragment) and
/// override innerHTML to read/write from content instead of element children.
fn install_template_element_props(
    scope: &mut v8::HandleScope,
    obj: v8::Local<v8::Object>,
    node_id: NodeId,
) {
    // Ensure template_contents exists (lazy init for createElement("template"))
    let content_id = {
        let arena = arena_mut(scope);
        if let NodeData::Element(data) = &arena.nodes[node_id].data {
            if let Some(id) = data.template_contents {
                id
            } else {
                // Create the content DocumentFragment
                let frag_id = arena.new_node(NodeData::DocumentFragment);
                if let NodeData::Element(data) = &mut arena.nodes[node_id].data {
                    data.template_contents = Some(frag_id);
                }
                frag_id
            }
        } else {
            return;
        }
    };

    // Wrap the content fragment
    let content_obj = wrap_node(scope, content_id);

    // Set .content as a property (returns the same fragment wrapper each time via WrapperCache)
    let k = v8::String::new(scope, "content").unwrap();
    obj.set(scope, k.into(), content_obj.into());

    // Override innerHTML to redirect to content fragment
    let getter = v8::Function::new(scope, template_innerhtml_getter).unwrap();
    let setter = v8::Function::new(scope, template_innerhtml_setter).unwrap();

    let source = v8::String::new(scope, r#"
    (function(obj, get, set) {
        Object.defineProperty(obj, "innerHTML", {
            get: function() { return get.call(this); },
            set: function(v) { return set.call(this, v); },
            configurable: true,
            enumerable: true
        });
    })
    "#).unwrap();
    let name = v8::String::new(scope, "[blazeweb:template-innerHTML]").unwrap();
    let origin = v8::ScriptOrigin::new(
        scope, name.into(), 0, 0, false, -1, None, false, false, false, None,
    );
    if let Some(script) = v8::Script::compile(scope, source, Some(&origin)) {
        if let Some(define_fn) = script.run(scope) {
            if define_fn.is_function() {
                let func = unsafe { v8::Local::<v8::Function>::cast_unchecked(define_fn) };
                let undef = v8::undefined(scope);
                func.call(scope, undef.into(), &[obj.into(), getter.into(), setter.into()]);
            }
        }
    }

    log::trace!("Installed HTMLTemplateElement .content + innerHTML redirect");
}

/// template.innerHTML getter — serializes content fragment children.
fn template_innerhtml_getter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    let k = v8::String::new(scope, "content").unwrap();
    let Some(content_val) = this.get(scope, k.into()) else { return };
    if !content_val.is_object() { return; }
    let content_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(content_val) };

    let Some(content_id) = unwrap_node_id(scope, content_obj) else { return };
    let arena = arena_ref(scope);
    let mut output = String::new();
    for child in arena.children(content_id) {
        crate::dom::serialize::serialize_node_to_string(arena, child, &mut output);
    }
    let v = v8::String::new(scope, &output).unwrap();
    rv.set(v.into());
}

/// template.innerHTML setter — parses HTML into content fragment.
fn template_innerhtml_setter(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let this = args.this();
    let html_str = args.get(0).to_rust_string_lossy(scope);

    let k = v8::String::new(scope, "content").unwrap();
    let Some(content_val) = this.get(scope, k.into()) else { return };
    if !content_val.is_object() { return; }
    let content_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(content_val) };
    let Some(content_id) = unwrap_node_id(scope, content_obj) else { return };

    // Parse the HTML fragment
    let fragment_arena = crate::dom::treesink::parse_fragment(&html_str, "template", true);

    let arena = arena_mut(scope);
    arena.remove_all_children(content_id);

    // Clone parsed nodes into content fragment
    if let Some(html_wrapper) = fragment_arena.children(fragment_arena.document).next() {
        for child in fragment_arena.children(html_wrapper) {
            let new_id = crate::js::bindings::element::clone_across_arenas(
                arena, &fragment_arena, child,
            );
            arena.append_child(content_id, new_id);
        }
    }
    log::trace!("template.innerHTML set ({} bytes)", html_str.len());
}
