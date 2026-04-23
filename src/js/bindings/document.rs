/// Document prototype bindings.
///
/// Installs accessors and methods specific to the Document interface.
/// Advanced methods (TreeWalker, Range, etc.) split into document_advanced.

use crate::dom::node::{ElementData, NodeData};
use crate::js::templates::{arena_mut, arena_ref, unwrap_node_id, wrap_node};
use super::helpers::{set_accessor, set_accessor_with_setter, set_method};
use super::document_advanced::{
    create_event, create_range, create_tree_walker, create_node_iterator,
    element_from_point, elements_from_point, document_get_selection,
    document_noop, document_exec_command, adopt_node, import_node,
};

pub fn install(scope: &mut v8::PinnedRef<v8::HandleScope<()>>, proto: &v8::Local<v8::ObjectTemplate>) {
    // Accessors
    set_accessor(scope, proto, "documentElement", document_element_getter);
    set_accessor(scope, proto, "head", head_getter);
    set_accessor_with_setter(scope, proto, "body", body_getter, body_setter);
    set_accessor_with_setter(scope, proto, "title", title_getter, title_setter);

    // Additional read-only accessors
    set_accessor(scope, proto, "readyState", ready_state_getter);
    set_accessor(scope, proto, "URL", document_url_getter);
    set_accessor(scope, proto, "baseURI", base_uri_getter);
    set_accessor(scope, proto, "documentURI", document_url_getter);

    // Read-write accessors
    set_accessor_with_setter(scope, proto, "cookie", cookie_getter, cookie_setter);

    // Batch 2: String-valued document accessors
    set_accessor(scope, proto, "characterSet", charset_getter);
    set_accessor(scope, proto, "charset", charset_getter);
    set_accessor(scope, proto, "inputEncoding", charset_getter);
    set_accessor(scope, proto, "compatMode", compat_mode_getter);
    set_accessor(scope, proto, "contentType", content_type_getter);
    set_accessor(scope, proto, "referrer", referrer_getter);
    set_accessor(scope, proto, "domain", domain_getter);
    set_accessor(scope, proto, "lastModified", last_modified_getter);
    set_accessor(scope, proto, "activeElement", active_element_getter);
    set_accessor(scope, proto, "currentScript", current_script_getter);
    set_accessor(scope, proto, "doctype", doctype_getter);
    set_accessor(scope, proto, "location", document_location_getter);
    set_accessor(scope, proto, "implementation", implementation_getter);

    // Collection-like accessors
    set_accessor(scope, proto, "forms", forms_getter);
    set_accessor(scope, proto, "images", images_getter);
    set_accessor(scope, proto, "links", links_getter);
    set_accessor(scope, proto, "scripts", scripts_getter);
    set_accessor(scope, proto, "anchors", anchors_getter);
    set_accessor(scope, proto, "all", all_getter);
    set_accessor(scope, proto, "styleSheets", empty_array_getter);
    set_accessor(scope, proto, "plugins", empty_array_getter);
    set_accessor(scope, proto, "embeds", empty_array_getter);

    // Methods
    set_method(scope, proto, "getElementById", get_element_by_id);
    set_method(scope, proto, "getElementsByTagName", get_elements_by_tag_name);
    set_method(scope, proto, "getElementsByClassName", get_elements_by_class_name);
    set_method(scope, proto, "createElement", create_element);
    set_method(scope, proto, "createElementNS", create_element_ns);
    set_method(scope, proto, "createTextNode", create_text_node);
    set_method(scope, proto, "createComment", create_comment);
    set_method(scope, proto, "createDocumentFragment", create_document_fragment);
    set_method(scope, proto, "querySelector", query_selector);
    set_method(scope, proto, "querySelectorAll", query_selector_all);
    set_method(scope, proto, "getElementsByName", get_elements_by_name);
    set_method(scope, proto, "createEvent", create_event);

    // Batch 2: New methods
    set_method(scope, proto, "hasFocus", has_focus);
    set_method(scope, proto, "createRange", create_range);
    set_method(scope, proto, "createTreeWalker", create_tree_walker);
    set_method(scope, proto, "createNodeIterator", create_node_iterator);
    set_method(scope, proto, "elementFromPoint", element_from_point);
    set_method(scope, proto, "elementsFromPoint", elements_from_point);
    set_method(scope, proto, "getSelection", document_get_selection);
    set_method(scope, proto, "open", document_noop);
    set_method(scope, proto, "close", document_noop);
    set_method(scope, proto, "write", document_noop);
    set_method(scope, proto, "writeln", document_noop);
    set_method(scope, proto, "execCommand", document_exec_command);
    set_method(scope, proto, "queryCommandSupported", document_exec_command);
    set_method(scope, proto, "adoptNode", adopt_node);
    set_method(scope, proto, "importNode", import_node);
}

// ─── Tree helpers ─────────────────────────────────────────────────────────────

/// Find the <html> element (first Element child of Document).
pub(super) fn find_document_element(arena: &crate::dom::Arena) -> Option<crate::dom::NodeId> {
    for child in arena.children(arena.document) {
        if let NodeData::Element(data) = &arena.nodes[child].data {
            if &*data.name.local == "html" {
                return Some(child);
            }
        }
    }
    None
}

/// Find a direct child element by tag name within a parent.
pub(super) fn find_child_element(
    arena: &crate::dom::Arena,
    parent: crate::dom::NodeId,
    tag: &str,
) -> Option<crate::dom::NodeId> {
    for child in arena.children(parent) {
        if let NodeData::Element(data) = &arena.nodes[child].data {
            if &*data.name.local == tag {
                return Some(child);
            }
        }
    }
    None
}

// ─── Accessors ────────────────────────────────────────────────────────────────

fn document_element_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let arena = arena_ref(scope);
    match find_document_element(arena) {
        Some(id) => rv.set(wrap_node(scope, id).into()),
        None => rv.set(v8::null(scope).into()),
    }
}

fn head_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let arena = arena_ref(scope);
    if let Some(html) = find_document_element(arena) {
        if let Some(head) = find_child_element(arena, html, "head") {
            rv.set(wrap_node(scope, head).into());
            return;
        }
    }
    rv.set(v8::null(scope).into());
}

fn body_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let arena = arena_ref(scope);
    if let Some(html) = find_document_element(arena) {
        if let Some(body) = find_child_element(arena, html, "body") {
            rv.set(wrap_node(scope, body).into());
            return;
        }
    }
    rv.set(v8::null(scope).into());
}

fn body_setter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let value = args.get(0);
    if !value.is_object() {
        return;
    }
    let obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(value) };
    let Some(new_body_id) = unwrap_node_id(scope, obj) else { return };

    let arena = arena_mut(scope);
    let Some(html) = find_document_element(arena) else { return };

    // Remove old body if present
    if let Some(old_body) = find_child_element(arena, html, "body") {
        arena.detach(old_body);
    }
    if arena.nodes[new_body_id].parent.is_some() {
        arena.detach(new_body_id);
    }
    arena.append_child(html, new_body_id);
}

fn title_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let arena = arena_ref(scope);
    if let Some(html) = find_document_element(arena) {
        if let Some(head) = find_child_element(arena, html, "head") {
            if let Some(title_el) = find_child_element(arena, head, "title") {
                // Collect text content of <title>
                let mut text = String::new();
                for child in arena.children(title_el) {
                    if let NodeData::Text(s) = &arena.nodes[child].data {
                        text.push_str(s);
                    }
                }
                let v8_str = v8::String::new(scope, text.trim()).unwrap();
                rv.set(v8_str.into());
                return;
            }
        }
    }
    let v8_str = v8::String::new(scope, "").unwrap();
    rv.set(v8_str.into());
}

fn title_setter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let text = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_mut(scope);
    let Some(html) = find_document_element(arena) else { return };
    let Some(head) = find_child_element(arena, html, "head") else { return };

    if let Some(title_el) = find_child_element(arena, head, "title") {
        arena.remove_all_children(title_el);
        let text_node = arena.new_node(NodeData::Text(text));
        arena.append_child(title_el, text_node);
    } else {
        // Create <title> element
        let name = markup5ever::QualName::new(None, markup5ever::ns!(html), "title".into());
        let title_el = arena.new_node(NodeData::Element(ElementData::new(name, vec![])));
        let text_node = arena.new_node(NodeData::Text(text));
        arena.append_child(title_el, text_node);
        arena.append_child(head, title_el);
    }
}

// ─── Methods ──────────────────────────────────────────────────────────────────

fn get_element_by_id(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let id_str = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_ref(scope);

    // Linear scan all nodes for matching id attribute
    for (node_id, node) in &arena.nodes {
        if let NodeData::Element(data) = &node.data {
            if data.get_attribute("id") == Some(&id_str) {
                let wrapped = wrap_node(scope, node_id);
                rv.set(wrapped.into());
                return;
            }
        }
    }
    rv.set(v8::null(scope).into());
}

fn get_elements_by_tag_name(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let tag = args.get(0).to_rust_string_lossy(scope).to_ascii_lowercase();
    let arena = arena_ref(scope);
    let mut results = Vec::new();

    collect_elements_by_tag(arena, arena.document, &tag, &mut results);

    let arr = v8::Array::new(scope, results.len() as i32);
    for (i, id) in results.iter().enumerate() {
        let wrapped = wrap_node(scope, *id);
        arr.set_index(scope, i as u32, wrapped.into());
    }
    super::element::add_item_method(scope, arr);
    rv.set(arr.into());
}

fn collect_elements_by_tag(
    arena: &crate::dom::Arena,
    node: crate::dom::NodeId,
    tag: &str,
    results: &mut Vec<crate::dom::NodeId>,
) {
    for child in arena.children(node) {
        if let NodeData::Element(data) = &arena.nodes[child].data {
            if tag == "*" || &*data.name.local == tag {
                results.push(child);
            }
        }
        collect_elements_by_tag(arena, child, tag, results);
    }
}

fn get_elements_by_class_name(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let class_names = args.get(0).to_rust_string_lossy(scope);
    let wanted: Vec<&str> = class_names.split_whitespace().collect();
    if wanted.is_empty() {
        let arr = v8::Array::new(scope, 0);
        rv.set(arr.into());
        return;
    }

    let arena = arena_ref(scope);
    let mut results = Vec::new();

    collect_elements_by_class(arena, arena.document, &wanted, &mut results);

    let arr = v8::Array::new(scope, results.len() as i32);
    for (i, id) in results.iter().enumerate() {
        let wrapped = wrap_node(scope, *id);
        arr.set_index(scope, i as u32, wrapped.into());
    }
    super::element::add_item_method(scope, arr);
    rv.set(arr.into());
}

fn collect_elements_by_class(
    arena: &crate::dom::Arena,
    node: crate::dom::NodeId,
    wanted: &[&str],
    results: &mut Vec<crate::dom::NodeId>,
) {
    for child in arena.children(node) {
        if let NodeData::Element(data) = &arena.nodes[child].data {
            if let Some(class_attr) = data.get_attribute("class") {
                let classes: Vec<&str> = class_attr.split_whitespace().collect();
                if wanted.iter().all(|w| classes.contains(w)) {
                    results.push(child);
                }
            }
        }
        collect_elements_by_class(arena, child, wanted, results);
    }
}

fn get_elements_by_name(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let name = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_ref(scope);
    let mut results = Vec::new();

    fn collect_by_name(
        arena: &crate::dom::Arena,
        node: crate::dom::NodeId,
        name: &str,
        results: &mut Vec<crate::dom::NodeId>,
    ) {
        for child in arena.children(node) {
            if let NodeData::Element(data) = &arena.nodes[child].data {
                if data.get_attribute("name") == Some(name) {
                    results.push(child);
                }
            }
            collect_by_name(arena, child, name, results);
        }
    }

    collect_by_name(arena, arena.document, &name, &mut results);

    let arr = v8::Array::new(scope, results.len() as i32);
    for (i, id) in results.iter().enumerate() {
        let wrapped = wrap_node(scope, *id);
        arr.set_index(scope, i as u32, wrapped.into());
    }
    rv.set(arr.into());
}

fn create_element(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let tag = args.get(0).to_rust_string_lossy(scope).to_ascii_lowercase();
    let arena = arena_mut(scope);
    let tag_str = tag.clone();
    let name = markup5ever::QualName::new(None, markup5ever::ns!(html), tag.into());
    let node_id = arena.new_node(NodeData::Element(ElementData::new(name, vec![])));
    let wrapped = wrap_node(scope, node_id);

    // Per spec: if tag name is a defined custom element, synchronously construct it.
    // Check if the tag contains a hyphen (custom element naming requirement) and is defined.
    if tag_str.contains('-') {
        let is_defined = scope
            .get_slot::<super::custom_elements::CustomElementState>()
            .map(|state| state.definitions.contains_key(&tag_str))
            .unwrap_or(false);
        if is_defined {
            // Run the custom element constructor via the construction stack.
            // This calls __ceUpgrade(element, Constructor) which:
            // 1. Pushes element onto construction stack
            // 2. Calls Reflect.construct(Constructor, [], Constructor)
            // 3. super() in the constructor returns the element from the stack
            // 4. Constructor body runs with `this` = the element
            let ctor_global = scope
                .get_slot::<super::custom_elements::CustomElementState>()
                .and_then(|state| state.definitions.get(&tag_str).map(|d| d.constructor.clone()));
            if let Some(ctor_g) = ctor_global {
                // Set prototype to Constructor.prototype
                let ctor_local = v8::Local::new(scope, &ctor_g);
                let proto_key = v8::String::new(scope, "prototype").unwrap();
                if let Some(ctor_proto) = ctor_local.get(scope, proto_key.into()) {
                    if ctor_proto.is_object() {
                        wrapped.set_prototype(scope, ctor_proto);
                    }
                }
                // Call __ceUpgrade(element, Constructor)
                let global = scope.get_current_context().global(scope);
                let upgrade_key = v8::String::new(scope, "__ceUpgrade").unwrap();
                if let Some(upgrade_fn_val) = global.get(scope, upgrade_key.into()) {
                    if upgrade_fn_val.is_function() {
                        let upgrade_fn = unsafe { v8::Local::<v8::Function>::cast_unchecked(upgrade_fn_val) };
                        let undef = v8::undefined(scope);
                        let ctor_local2 = v8::Local::new(scope, &ctor_g);
                        crate::try_catch!(let try_catch, scope);
                        if upgrade_fn.call(try_catch, undef.into(), &[wrapped.into(), ctor_local2.into()]).is_none() {
                            if let Some(exc) = try_catch.exception() {
                                log::warn!(
                                    "Custom element construction failed for <{}>: {}",
                                    tag_str, exc.to_rust_string_lossy(try_catch)
                                );
                            }
                        }
                    }
                }
            }
            log::trace!("createElement('{}') — synchronous custom element construction", tag_str);
        }
    }

    rv.set(wrapped.into());
}

fn create_element_ns(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let ns_arg = args.get(0);
    let qualified = args.get(1).to_rust_string_lossy(scope);

    let namespace = if ns_arg.is_null() || ns_arg.is_undefined() {
        markup5ever::ns!()
    } else {
        let ns_str = ns_arg.to_rust_string_lossy(scope);
        match ns_str.as_str() {
            "http://www.w3.org/1999/xhtml" => markup5ever::ns!(html),
            "http://www.w3.org/2000/svg" => markup5ever::ns!(svg),
            "http://www.w3.org/1998/Math/MathML" => markup5ever::ns!(mathml),
            other => markup5ever::Namespace::from(other),
        }
    };

    // Per DOM spec: createElementNS preserves caller's case (no lowercasing).
    // Only createElement lowercases for HTML documents.
    let (prefix, local) = if let Some(idx) = qualified.find(':') {
        let p = &qualified[..idx];
        let l = &qualified[idx + 1..];
        (Some(markup5ever::Prefix::from(p)), l.to_string())
    } else {
        (None, qualified.to_string())
    };

    let name = markup5ever::QualName::new(prefix, namespace, local.into());
    let arena = arena_mut(scope);
    let node_id = arena.new_node(NodeData::Element(ElementData::new(name, vec![])));
    let wrapped = wrap_node(scope, node_id);
    rv.set(wrapped.into());
}

fn create_text_node(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let text = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_mut(scope);
    let node_id = arena.new_node(NodeData::Text(text));
    let wrapped = wrap_node(scope, node_id);
    rv.set(wrapped.into());
}

fn create_comment(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let text = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_mut(scope);
    let node_id = arena.new_node(NodeData::Comment(text));
    let wrapped = wrap_node(scope, node_id);
    rv.set(wrapped.into());
}

fn create_document_fragment(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let arena = arena_mut(scope);
    let node_id = arena.new_node(NodeData::DocumentFragment);
    let wrapped = wrap_node(scope, node_id);
    rv.set(wrapped.into());
}

pub struct DocumentCookie(pub String);

/// Tracks the currently executing <script> element for document.currentScript.
pub struct CurrentScriptId(pub Option<crate::dom::NodeId>);

fn ready_state_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let v = v8::String::new(scope, "complete").unwrap();
    rv.set(v.into());
}

fn document_url_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let url = scope.get_slot::<super::location::BaseUrl>()
        .and_then(|b| b.0.as_ref().cloned())
        .unwrap_or_else(|| "about:blank".to_string());
    let v = v8::String::new(scope, &url).unwrap();
    rv.set(v.into());
}

fn base_uri_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let url = scope.get_slot::<super::location::BaseUrl>()
        .and_then(|b| b.0.as_ref().cloned())
        .unwrap_or_else(|| "about:blank".to_string());
    let v = v8::String::new(scope, &url).unwrap();
    rv.set(v.into());
}

fn cookie_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let cookie = scope.get_slot::<DocumentCookie>()
        .map(|c| c.0.clone())
        .unwrap_or_default();
    let v = v8::String::new(scope, &cookie).unwrap();
    rv.set(v.into());
}

fn cookie_setter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let val = args.get(0).to_rust_string_lossy(scope);
    if let Some(cookie) = scope.get_slot_mut::<DocumentCookie>() {
        cookie.0 = val;
    }
}

fn query_selector(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let selector = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_ref(scope);
    match crate::dom::selector::query_selector(arena, arena.document, &selector) {
        Ok(Some(id)) => rv.set(wrap_node(scope, id).into()),
        Ok(None) | Err(_) => rv.set(v8::null(scope).into()),
    }
}

fn query_selector_all(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let selector = args.get(0).to_rust_string_lossy(scope);
    let arena = arena_ref(scope);
    match crate::dom::selector::query_selector_all(arena, arena.document, &selector) {
        Ok(ids) => {
            let arr = v8::Array::new(scope, ids.len() as i32);
            for (i, id) in ids.iter().enumerate() {
                let wrapped = wrap_node(scope, *id);
                arr.set_index(scope, i as u32, wrapped.into());
            }
            rv.set(arr.into());
        }
        Err(_) => {
            rv.set(v8::Array::new(scope, 0).into());
        }
    }
}

// ─── Batch 2: Document string accessors ──────────────────────────────────────

fn charset_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    rv.set(v8::String::new(scope, "UTF-8").unwrap().into());
}

fn compat_mode_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    rv.set(v8::String::new(scope, "CSS1Compat").unwrap().into());
}

fn content_type_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    rv.set(v8::String::new(scope, "text/html").unwrap().into());
}

fn referrer_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    rv.set(v8::String::new(scope, "").unwrap().into());
}

fn domain_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let domain = scope.get_slot::<super::location::BaseUrl>()
        .and_then(|b| b.0.as_ref().and_then(|u| url::Url::parse(u).ok()))
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_default();
    rv.set(v8::String::new(scope, &domain).unwrap().into());
}

fn last_modified_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    rv.set(v8::String::new(scope, "01/01/2026 00:00:00").unwrap().into());
}

fn current_script_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if let Some(cs) = scope.get_slot::<CurrentScriptId>() {
        if let Some(node_id) = cs.0 {
            rv.set(wrap_node(scope, node_id).into());
            return;
        }
    }
    rv.set(v8::null(scope).into());
}

fn active_element_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let arena = arena_ref(scope);
    if let Some(html) = find_document_element(arena) {
        if let Some(body) = find_child_element(arena, html, "body") {
            rv.set(wrap_node(scope, body).into());
            return;
        }
    }
    rv.set(v8::null(scope).into());
}

fn doctype_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let arena = arena_ref(scope);
    // Find the Doctype node
    for child in arena.children(arena.document) {
        if let NodeData::Doctype { name, .. } = &arena.nodes[child].data {
            let obj = v8::Object::new(scope);
            let k = v8::String::new(scope, "name").unwrap();
            let v = v8::String::new(scope, name).unwrap();
            obj.set(scope, k.into(), v.into());
            let empty = v8::String::new(scope, "").unwrap();
            let k = v8::String::new(scope, "publicId").unwrap();
            obj.set(scope, k.into(), empty.into());
            let k = v8::String::new(scope, "systemId").unwrap();
            obj.set(scope, k.into(), empty.into());
            rv.set(obj.into());
            return;
        }
    }
    rv.set(v8::null(scope).into());
}

/// document.location — returns the same Location object as window.location.
fn document_location_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let global = scope.get_current_context().global(scope);
    let k = v8::String::new(scope, "location").unwrap();
    if let Some(loc) = global.get(scope, k.into()) {
        rv.set(loc);
    }
}

fn implementation_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let obj = v8::Object::new(scope);
    let create_html_doc = v8::Function::new(scope, |scope: &mut v8::PinnedRef<v8::HandleScope>, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        // Return a minimal Document-like object
        let doc = v8::Object::new(scope);
        let k = v8::String::new(scope, "body").unwrap();
        let body = v8::Object::new(scope);
        let k2 = v8::String::new(scope, "innerHTML").unwrap();
        let v2 = v8::String::new(scope, "").unwrap();
        body.set(scope, k2.into(), v2.into());
        doc.set(scope, k.into(), body.into());
        rv.set(doc.into());
    }).unwrap();
    let k = v8::String::new(scope, "createHTMLDocument").unwrap();
    obj.set(scope, k.into(), create_html_doc.into());
    let has_feature = v8::Function::new(scope, |scope: &mut v8::PinnedRef<v8::HandleScope>, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        rv.set(v8::Boolean::new(scope, true).into());
    }).unwrap();
    let k = v8::String::new(scope, "hasFeature").unwrap();
    obj.set(scope, k.into(), has_feature.into());
    rv.set(obj.into());
}

// ─── Collection accessors ────────────────────────────────────────────────────

fn forms_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    collect_elements_by_tag_global(scope, "form", &mut rv);
}

fn images_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    collect_elements_by_tag_global(scope, "img", &mut rv);
}

fn links_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let arena = arena_ref(scope);
    let mut results = Vec::new();
    collect_links(arena, arena.document, &mut results);
    let arr = v8::Array::new(scope, results.len() as i32);
    for (i, id) in results.iter().enumerate() {
        let wrapped = wrap_node(scope, *id);
        arr.set_index(scope, i as u32, wrapped.into());
    }
    rv.set(arr.into());
}

fn collect_links(
    arena: &crate::dom::Arena,
    node: crate::dom::NodeId,
    results: &mut Vec<crate::dom::NodeId>,
) {
    for child in arena.children(node) {
        if let NodeData::Element(data) = &arena.nodes[child].data {
            let tag = &*data.name.local;
            if (tag == "a" || tag == "area") && data.get_attribute("href").is_some() {
                results.push(child);
            }
        }
        collect_links(arena, child, results);
    }
}

fn scripts_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    collect_elements_by_tag_global(scope, "script", &mut rv);
}

fn anchors_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let arena = arena_ref(scope);
    let mut results = Vec::new();
    collect_anchors(arena, arena.document, &mut results);
    let arr = v8::Array::new(scope, results.len() as i32);
    for (i, id) in results.iter().enumerate() {
        let wrapped = wrap_node(scope, *id);
        arr.set_index(scope, i as u32, wrapped.into());
    }
    rv.set(arr.into());
}

fn collect_anchors(
    arena: &crate::dom::Arena,
    node: crate::dom::NodeId,
    results: &mut Vec<crate::dom::NodeId>,
) {
    for child in arena.children(node) {
        if let NodeData::Element(data) = &arena.nodes[child].data {
            if &*data.name.local == "a" && data.get_attribute("name").is_some() {
                results.push(child);
            }
        }
        collect_anchors(arena, child, results);
    }
}

fn all_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    collect_elements_by_tag_global(scope, "*", &mut rv);
}

fn empty_array_getter(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    rv.set(v8::Array::new(scope, 0).into());
}

fn collect_elements_by_tag_global(scope: &mut v8::PinnedRef<v8::HandleScope>, tag: &str, rv: &mut v8::ReturnValue) {
    let arena = arena_ref(scope);
    let mut results = Vec::new();
    collect_elements_by_tag(arena, arena.document, tag, &mut results);
    let arr = v8::Array::new(scope, results.len() as i32);
    for (i, id) in results.iter().enumerate() {
        let wrapped = wrap_node(scope, *id);
        arr.set_index(scope, i as u32, wrapped.into());
    }
    rv.set(arr.into());
}

// ─── Batch 2: New methods ────────────────────────────────────────────────────

fn has_focus(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    rv.set(v8::Boolean::new(scope, true).into());
}

