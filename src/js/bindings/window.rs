/// Window/global setup — thin coordinator.
///
/// Installs document, window, self, and window-specific APIs on the global,
/// then delegates to feature modules for constructors and utilities.

use crate::js::templates::{arena_ref, wrap_node};

/// Install global objects on the context's global object.
/// Must be called after context creation (inside ContextScope).
pub fn install_globals(scope: &mut v8::HandleScope) {
    let context = scope.get_current_context();
    let global = context.global(scope);

    // ─── Core window objects ─────────────────────────────────────────────

    // document = the arena's Document node
    let arena = arena_ref(scope);
    let doc_id = arena.document;
    let doc_obj = wrap_node(scope, doc_id);
    let key = v8::String::new(scope, "document").unwrap();
    global.set(scope, key.into(), doc_obj.into());

    // window = self = globalThis
    for name in &["window", "self"] {
        let key = v8::String::new(scope, name).unwrap();
        global.set(scope, key.into(), global.into());
    }

    // console
    super::console::install(scope, global);

    // Timer APIs (setTimeout, setInterval, clearTimeout, clearInterval,
    // requestAnimationFrame, cancelAnimationFrame)
    crate::js::timers::install(scope);

    // Event APIs on window
    let add_el = v8::Function::new(scope, crate::js::events::window_add_event_listener).unwrap();
    let key = v8::String::new(scope, "addEventListener").unwrap();
    global.set(scope, key.into(), add_el.into());

    let remove_el = v8::Function::new(scope, crate::js::events::window_remove_event_listener).unwrap();
    let key = v8::String::new(scope, "removeEventListener").unwrap();
    global.set(scope, key.into(), remove_el.into());

    let dispatch = v8::Function::new(scope, crate::js::events::window_dispatch_event_callback).unwrap();
    let key = v8::String::new(scope, "dispatchEvent").unwrap();
    global.set(scope, key.into(), dispatch.into());

    // location
    let location = super::location::create_location_object(scope);
    let key = v8::String::new(scope, "location").unwrap();
    global.set(scope, key.into(), location.into());

    // navigator
    let navigator = super::navigator::create_navigator_object(scope);
    let key = v8::String::new(scope, "navigator").unwrap();
    global.set(scope, key.into(), navigator.into());

    // localStorage / sessionStorage
    let local_storage = super::storage::create_storage_object(scope, true);
    let key = v8::String::new(scope, "localStorage").unwrap();
    global.set(scope, key.into(), local_storage.into());

    let session_storage = super::storage::create_storage_object(scope, false);
    let key = v8::String::new(scope, "sessionStorage").unwrap();
    global.set(scope, key.into(), session_storage.into());

    // getComputedStyle
    let gcs = v8::Function::new(scope, get_computed_style).unwrap();
    let key = v8::String::new(scope, "getComputedStyle").unwrap();
    global.set(scope, key.into(), gcs.into());

    // fetch — real implementation with batched concurrent HTTP
    crate::js::fetch::install(scope);

    // ─── Feature modules ─────────────────────────────────────────────────

    super::url::install(scope, global);
    super::event_constructors::install(scope, global);
    super::observers::install(scope, global);
    super::xhr::install(scope, global);
    super::encoding::install(scope, global);
    super::dom_parser::install(scope, global);
    super::abort_controller::install(scope, global);
    super::messaging::install(scope, global);
    super::custom_elements::install(scope, global);
    super::html_constructors::install(scope, global);
    super::formdata::install(scope, global);
    super::headers::install(scope, global);
    super::blob::install(scope, global);
    super::crypto::install(scope, global);

    // ─── Window-specific APIs ────────────────────────────────────────────

    // queueMicrotask
    let qmt = v8::Function::new(scope, queue_microtask).unwrap();
    let key = v8::String::new(scope, "queueMicrotask").unwrap();
    global.set(scope, key.into(), qmt.into());

    // matchMedia
    let mm = v8::Function::new(scope, match_media).unwrap();
    let key = v8::String::new(scope, "matchMedia").unwrap();
    global.set(scope, key.into(), mm.into());

    // performance stub
    let perf = v8::Object::new(scope);
    let now_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        rv.set(v8::Number::new(scope, 0.0).into());
    }).unwrap();
    let k = v8::String::new(scope, "now").unwrap();
    perf.set(scope, k.into(), now_fn.into());
    let get_entries = v8::Function::new(scope, |scope: &mut v8::HandleScope, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        rv.set(v8::Array::new(scope, 0).into());
    }).unwrap();
    for name in &["getEntriesByType", "getEntriesByName", "getEntries"] {
        let k = v8::String::new(scope, name).unwrap();
        perf.set(scope, k.into(), get_entries.into());
    }
    let noop_fn = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    for name in &["mark", "measure", "clearMarks", "clearMeasures"] {
        let k = v8::String::new(scope, name).unwrap();
        perf.set(scope, k.into(), noop_fn.into());
    }
    let timing = v8::Object::new(scope);
    let zero = v8::Number::new(scope, 0.0);
    for name in &["navigationStart", "domContentLoadedEventEnd", "loadEventEnd"] {
        let k = v8::String::new(scope, name).unwrap();
        timing.set(scope, k.into(), zero.into());
    }
    let k = v8::String::new(scope, "timing").unwrap();
    perf.set(scope, k.into(), timing.into());
    let key = v8::String::new(scope, "performance").unwrap();
    global.set(scope, key.into(), perf.into());

    // requestIdleCallback / cancelIdleCallback
    let ric = v8::Function::new(scope, request_idle_callback).unwrap();
    let key = v8::String::new(scope, "requestIdleCallback").unwrap();
    global.set(scope, key.into(), ric.into());
    let cic = v8::Function::new(scope, |scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue| {
        let id = args.get(0).int32_value(scope).unwrap_or(0) as u32;
        if let Some(queue) = scope.get_slot_mut::<crate::js::timers::TimerQueue>() {
            queue.remove(id);
        }
    }).unwrap();
    let key = v8::String::new(scope, "cancelIdleCallback").unwrap();
    global.set(scope, key.into(), cic.into());

    // scrollTo / scrollBy / scroll — no-op
    let scroll_noop = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    for name in &["scrollTo", "scrollBy", "scroll"] {
        let key = v8::String::new(scope, name).unwrap();
        global.set(scope, key.into(), scroll_noop.into());
    }

    // Dimension properties
    let set_int = |scope: &mut v8::HandleScope, obj: v8::Local<v8::Object>, name: &str, val: i32| {
        let k = v8::String::new(scope, name).unwrap();
        let v = v8::Integer::new(scope, val);
        obj.set(scope, k.into(), v.into());
    };
    set_int(scope, global, "innerWidth", 1920);
    set_int(scope, global, "outerWidth", 1920);
    set_int(scope, global, "innerHeight", 1080);
    set_int(scope, global, "outerHeight", 1080);
    set_int(scope, global, "scrollX", 0);
    set_int(scope, global, "scrollY", 0);
    set_int(scope, global, "pageXOffset", 0);
    set_int(scope, global, "pageYOffset", 0);

    // devicePixelRatio
    let k = v8::String::new(scope, "devicePixelRatio").unwrap();
    let v = v8::Number::new(scope, 1.0);
    global.set(scope, k.into(), v.into());

    // isSecureContext
    let k = v8::String::new(scope, "isSecureContext").unwrap();
    let v = v8::Boolean::new(scope, true);
    global.set(scope, k.into(), v.into());

    // origin
    let origin_str = scope.get_slot::<super::location::BaseUrl>()
        .and_then(|b| b.0.as_ref().and_then(|u| reqwest::Url::parse(u).ok()))
        .map(|u| format!("{}://{}", u.scheme(), u.host_str().unwrap_or("")))
        .unwrap_or_else(|| "null".to_string());
    let k = v8::String::new(scope, "origin").unwrap();
    let v = v8::String::new(scope, &origin_str).unwrap();
    global.set(scope, k.into(), v.into());

    // screen object
    let screen = v8::Object::new(scope);
    set_int(scope, screen, "width", 1920);
    set_int(scope, screen, "height", 1080);
    set_int(scope, screen, "availWidth", 1920);
    set_int(scope, screen, "availHeight", 1080);
    set_int(scope, screen, "colorDepth", 24);
    set_int(scope, screen, "pixelDepth", 24);
    let k = v8::String::new(scope, "orientation").unwrap();
    let orientation = v8::Object::new(scope);
    let k2 = v8::String::new(scope, "type").unwrap();
    let v2 = v8::String::new(scope, "landscape-primary").unwrap();
    orientation.set(scope, k2.into(), v2.into());
    let k2 = v8::String::new(scope, "angle").unwrap();
    let v2 = v8::Integer::new(scope, 0);
    orientation.set(scope, k2.into(), v2.into());
    screen.set(scope, k.into(), orientation.into());
    let key = v8::String::new(scope, "screen").unwrap();
    global.set(scope, key.into(), screen.into());

    // history object
    let history = v8::Object::new(scope);
    set_int(scope, history, "length", 1);
    let k = v8::String::new(scope, "state").unwrap();
    let null = v8::null(scope);
    history.set(scope, k.into(), null.into());
    let history_noop = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    for name in &["pushState", "replaceState", "back", "forward", "go"] {
        let k = v8::String::new(scope, name).unwrap();
        history.set(scope, k.into(), history_noop.into());
    }
    let key = v8::String::new(scope, "history").unwrap();
    global.set(scope, key.into(), history.into());

    // getSelection()
    let get_sel = v8::Function::new(scope, get_selection).unwrap();
    let key = v8::String::new(scope, "getSelection").unwrap();
    global.set(scope, key.into(), get_sel.into());

    // visualViewport
    let vv = v8::Object::new(scope);
    set_int(scope, vv, "width", 1920);
    set_int(scope, vv, "height", 1080);
    let k = v8::String::new(scope, "scale").unwrap();
    let v = v8::Number::new(scope, 1.0);
    vv.set(scope, k.into(), v.into());
    set_int(scope, vv, "offsetLeft", 0);
    set_int(scope, vv, "offsetTop", 0);
    let vv_noop = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    for name in &["addEventListener", "removeEventListener"] {
        let k = v8::String::new(scope, name).unwrap();
        vv.set(scope, k.into(), vv_noop.into());
    }
    let key = v8::String::new(scope, "visualViewport").unwrap();
    global.set(scope, key.into(), vv.into());

    // structuredClone
    let sc = v8::Function::new(scope, structured_clone).unwrap();
    let key = v8::String::new(scope, "structuredClone").unwrap();
    global.set(scope, key.into(), sc.into());

    // ─── Self-references and edge cases ──────────────────────────────────

    // parent / top / frames = self
    for name in &["parent", "top", "frames"] {
        let key = v8::String::new(scope, name).unwrap();
        global.set(scope, key.into(), global.into());
    }
    // opener = null
    let k = v8::String::new(scope, "opener").unwrap();
    let null = v8::null(scope);
    global.set(scope, k.into(), null.into());
    // closed = false
    let k = v8::String::new(scope, "closed").unwrap();
    let v = v8::Boolean::new(scope, false);
    global.set(scope, k.into(), v.into());
    // name = ""
    let k = v8::String::new(scope, "name").unwrap();
    let v = v8::String::new(scope, "").unwrap();
    global.set(scope, k.into(), v.into());
    // frameElement = null
    let k = v8::String::new(scope, "frameElement").unwrap();
    let null = v8::null(scope);
    global.set(scope, k.into(), null.into());
    // length = 0 (number of frames)
    let k = v8::String::new(scope, "length").unwrap();
    let v = v8::Integer::new(scope, 0);
    global.set(scope, k.into(), v.into());

    // statusbar / menubar / toolbar / personalbar / scrollbars / locationbar
    for name in &["statusbar", "menubar", "toolbar", "personalbar", "scrollbars", "locationbar"] {
        let bar = v8::Object::new(scope);
        let k2 = v8::String::new(scope, "visible").unwrap();
        let v2 = v8::Boolean::new(scope, true);
        bar.set(scope, k2.into(), v2.into());
        let key = v8::String::new(scope, name).unwrap();
        global.set(scope, key.into(), bar.into());
    }

    // ─── No-op constructors for type checking ────────────────────────────

    let noop_ctor = v8::Function::new(scope, |_scope: &mut v8::HandleScope, _args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue| {}).unwrap();
    for name in &[
        "HTMLElement", "HTMLDivElement", "HTMLSpanElement", "HTMLInputElement",
        "HTMLFormElement", "HTMLAnchorElement", "HTMLImageElement",
        "HTMLButtonElement", "HTMLSelectElement", "HTMLOptionElement",
        "HTMLTextAreaElement", "HTMLScriptElement", "HTMLStyleElement",
        "HTMLLinkElement", "HTMLMetaElement", "HTMLHeadElement",
        "HTMLBodyElement", "HTMLHtmlElement", "HTMLDocument",
        "HTMLTemplateElement", "HTMLCanvasElement", "HTMLVideoElement",
        "HTMLAudioElement", "HTMLMediaElement", "HTMLIFrameElement",
        "HTMLTableElement", "HTMLTableRowElement", "HTMLTableCellElement",
        "HTMLParagraphElement", "HTMLPreElement", "HTMLUListElement",
        "HTMLOListElement", "HTMLLIElement", "HTMLLabelElement",
        "HTMLFieldSetElement", "HTMLLegendElement", "HTMLProgressElement",
        "HTMLDialogElement", "HTMLDetailsElement", "HTMLSlotElement",
        "HTMLPictureElement", "HTMLSourceElement", "HTMLTrackElement",
        "Node", "Element", "Document", "DocumentFragment", "Text", "Comment",
        "WebSocket", "File", "FileReader",
        "Request", "Response",
        "AbortSignal", "XMLSerializer", "NodeList", "HTMLCollection",
        "DOMTokenList", "CSSStyleDeclaration", "NamedNodeMap", "DOMRect",
        "NodeFilter", "TreeWalker", "Range", "Selection",
        "PerformanceObserver", "ReportingObserver",
        "Proxy", "WeakRef", "FinalizationRegistry",
        "BroadcastChannel", "MessagePort",
        "CSSStyleSheet", "StyleSheet", "MediaQueryList",
    ] {
        let key = v8::String::new(scope, name).unwrap();
        // Only set if not already defined (functional constructors take priority)
        if global.get(scope, key.into()).map(|v| v.is_undefined()).unwrap_or(true) {
            global.set(scope, key.into(), noop_ctor.into());
        }
    }

    // NodeFilter constants — WHATWG DOM §6 NodeFilter interface
    {
        let nf = v8::Object::new(scope);
        let set_const = |scope: &mut v8::HandleScope, obj: v8::Local<v8::Object>, name: &str, val: u32| {
            let k = v8::String::new(scope, name).unwrap();
            let v = v8::Integer::new(scope, val as i32);
            obj.set(scope, k.into(), v.into());
        };
        set_const(scope, nf, "FILTER_ACCEPT", 1);
        set_const(scope, nf, "FILTER_REJECT", 2);
        set_const(scope, nf, "FILTER_SKIP", 3);
        // SHOW_ALL is 0xFFFFFFFF — must use Number (not Integer) to preserve unsigned value
        let k = v8::String::new(scope, "SHOW_ALL").unwrap();
        let v = v8::Number::new(scope, 0xFFFFFFFF_u32 as f64);
        nf.set(scope, k.into(), v.into());
        set_const(scope, nf, "SHOW_ELEMENT", 0x1);
        set_const(scope, nf, "SHOW_ATTRIBUTE", 0x2);
        set_const(scope, nf, "SHOW_TEXT", 0x4);
        set_const(scope, nf, "SHOW_CDATA_SECTION", 0x8);
        set_const(scope, nf, "SHOW_PROCESSING_INSTRUCTION", 0x40);
        set_const(scope, nf, "SHOW_COMMENT", 0x80);
        set_const(scope, nf, "SHOW_DOCUMENT", 0x100);
        set_const(scope, nf, "SHOW_DOCUMENT_TYPE", 0x200);
        set_const(scope, nf, "SHOW_DOCUMENT_FRAGMENT", 0x400);
        let k = v8::String::new(scope, "NodeFilter").unwrap();
        global.set(scope, k.into(), nf.into());
    }
}

// ─── Window-specific functions ───────────────────────────────────────────────

fn get_computed_style(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let obj = v8::Object::new(scope);

    // Try to read inline styles from the element
    let el_arg = args.get(0);
    let mut inline_styles: Vec<(String, String)> = Vec::new();
    if el_arg.is_object() {
        let el_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(el_arg) };
        if let Some(node_id) = crate::js::templates::unwrap_node_id(scope, el_obj) {
            let arena = arena_ref(scope);
            if let crate::dom::node::NodeData::Element(data) = &arena.nodes[node_id].data {
                if let Some(style_str) = data.get_attribute("style") {
                    for declaration in style_str.split(';') {
                        let declaration = declaration.trim();
                        if let Some((prop, val)) = declaration.split_once(':') {
                            inline_styles.push((prop.trim().to_string(), val.trim().to_string()));
                        }
                    }
                }
            }
        }
    }

    // Set inline styles on the computed style object
    for (prop, val) in &inline_styles {
        let k = v8::String::new(scope, prop).unwrap();
        let v = v8::String::new(scope, val).unwrap();
        obj.set(scope, k.into(), v.into());
        // Also set camelCase version
        let camel = css_to_camel(prop);
        if camel != *prop {
            let k = v8::String::new(scope, &camel).unwrap();
            obj.set(scope, k.into(), v.into());
        }
    }

    // getPropertyValue reads from inline styles
    let gpv = v8::Function::new(scope, |scope: &mut v8::HandleScope, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let v = v8::String::new(scope, "").unwrap();
        rv.set(v.into());
    }).unwrap();
    let k = v8::String::new(scope, "getPropertyValue").unwrap();
    obj.set(scope, k.into(), gpv.into());

    // setProperty / removeProperty — no-op
    let noop = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    let k = v8::String::new(scope, "setProperty").unwrap();
    obj.set(scope, k.into(), noop.into());
    let k = v8::String::new(scope, "removeProperty").unwrap();
    obj.set(scope, k.into(), noop.into());

    // Set common defaults for properties not in inline styles
    let empty = v8::String::new(scope, "").unwrap();
    let common_props = ["display", "visibility", "position", "overflow", "color",
                         "backgroundColor", "fontSize", "fontFamily", "fontWeight",
                         "margin", "padding", "border", "width", "height",
                         "top", "left", "right", "bottom", "zIndex", "opacity",
                         "transform", "transition", "float", "clear",
                         "textAlign", "textDecoration", "lineHeight", "letterSpacing",
                         "boxSizing", "cursor", "pointerEvents"];
    for prop in &common_props {
        let k = v8::String::new(scope, prop).unwrap();
        if obj.get(scope, k.into()).map(|v| v.is_undefined()).unwrap_or(true) {
            obj.set(scope, k.into(), empty.into());
        }
    }

    // length = 0
    let k = v8::String::new(scope, "length").unwrap();
    let val = v8::Integer::new(scope, 0);
    obj.set(scope, k.into(), val.into());

    rv.set(obj.into());
}

fn css_to_camel(prop: &str) -> String {
    let mut result = String::with_capacity(prop.len());
    let mut capitalize_next = false;
    for c in prop.chars() {
        if c == '-' {
            capitalize_next = true;
        } else if capitalize_next {
            result.extend(c.to_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    result
}

fn queue_microtask(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let callback = args.get(0);
    if callback.is_function() {
        let func = unsafe { v8::Local::<v8::Function>::cast_unchecked(callback) };
        scope.enqueue_microtask(func);
    }
}

fn match_media(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let query = args.get(0).to_rust_string_lossy(scope);
    let obj = v8::Object::new(scope);

    // Parse common media queries for sensible SSR defaults
    let matches = evaluate_media_query(&query);
    let k = v8::String::new(scope, "matches").unwrap();
    let v = v8::Boolean::new(scope, matches);
    obj.set(scope, k.into(), v.into());

    let k = v8::String::new(scope, "media").unwrap();
    let v = v8::String::new(scope, &query).unwrap();
    obj.set(scope, k.into(), v.into());

    let noop = v8::Function::new(scope, |_scope: &mut v8::HandleScope, _args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue| {}).unwrap();
    for name in &["addEventListener", "removeEventListener", "addListener", "removeListener", "dispatchEvent"] {
        let k = v8::String::new(scope, name).unwrap();
        obj.set(scope, k.into(), noop.into());
    }

    // onchange settable property
    let k = v8::String::new(scope, "onchange").unwrap();
    let val = v8::null(scope);
    obj.set(scope, k.into(), val.into());

    rv.set(obj.into());
}

fn evaluate_media_query(query: &str) -> bool {
    let q = query.trim().to_ascii_lowercase();
    if q == "screen" || q == "all" || q == "(screen)" || q == "all and (min-width: 0px)" {
        return true;
    }
    if q == "print" || q.starts_with("print ") {
        return false;
    }
    if q.contains("prefers-color-scheme: light") || q.contains("prefers-color-scheme:light") {
        return true;
    }
    if q.contains("prefers-color-scheme: dark") || q.contains("prefers-color-scheme:dark") {
        return false;
    }
    if q.contains("prefers-reduced-motion: no-preference") {
        return true;
    }
    if q.contains("prefers-reduced-motion: reduce") {
        return false;
    }
    if let Some(val) = extract_px_value(&q, "min-width") {
        return val <= 1920.0;
    }
    if let Some(val) = extract_px_value(&q, "max-width") {
        return val >= 1920.0;
    }
    if let Some(val) = extract_px_value(&q, "min-height") {
        return val <= 1080.0;
    }
    if let Some(val) = extract_px_value(&q, "max-height") {
        return val >= 1080.0;
    }
    if q.contains("pointer: fine") || q.contains("pointer:fine") {
        return true;
    }
    if q.contains("hover: hover") || q.contains("hover:hover") {
        return true;
    }
    false
}

fn extract_px_value(query: &str, prop: &str) -> Option<f64> {
    let idx = query.find(prop)?;
    let rest = &query[idx + prop.len()..];
    let rest = rest.trim_start().strip_prefix(':')?.trim_start();
    let end = rest.find("px").unwrap_or(rest.len());
    let num_str = rest[..end].trim();
    num_str.parse::<f64>().ok()
}

fn request_idle_callback(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let callback_arg = args.get(0);
    if !callback_arg.is_function() {
        rv.set(v8::Integer::new(scope, 0).into());
        return;
    }
    let func = unsafe { v8::Local::<v8::Function>::cast_unchecked(callback_arg) };
    let global_func = v8::Global::new(scope, func);
    let queue = scope.get_slot_mut::<crate::js::timers::TimerQueue>().unwrap();
    let id = queue.add(global_func, 0, false);
    rv.set(v8::Integer::new(scope, id as i32).into());
}

fn get_selection(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let obj = v8::Object::new(scope);
    let k = v8::String::new(scope, "rangeCount").unwrap();
    let v = v8::Integer::new(scope, 0);
    obj.set(scope, k.into(), v.into());
    let k = v8::String::new(scope, "isCollapsed").unwrap();
    let v = v8::Boolean::new(scope, true);
    obj.set(scope, k.into(), v.into());
    let to_str = v8::Function::new(scope, |scope: &mut v8::HandleScope, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        rv.set(v8::String::new(scope, "").unwrap().into());
    }).unwrap();
    let k = v8::String::new(scope, "toString").unwrap();
    obj.set(scope, k.into(), to_str.into());
    let noop = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    for name in &["removeAllRanges", "addRange", "collapse", "collapseToStart", "collapseToEnd",
                   "extend", "setBaseAndExtent", "selectAllChildren", "deleteFromDocument",
                   "getRangeAt", "containsNode"] {
        let k = v8::String::new(scope, name).unwrap();
        obj.set(scope, k.into(), noop.into());
    }
    rv.set(obj.into());
}

fn structured_clone(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let val = args.get(0);
    if let Some(json_str) = v8::json::stringify(scope, val) {
        if let Some(parsed) = v8::json::parse(scope, json_str) {
            rv.set(parsed);
            return;
        }
    }
    rv.set(val);
}
