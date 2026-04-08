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
    for name in &["window", "self", "globalThis"] {
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
    super::streams::install(scope, global);
    super::canvas::install_globals(scope, global);
    install_cssstylesheet(scope, global);
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

    // performance — real mark/measure that queue to PerformanceObserver
    let perf = v8::Object::new(scope);
    let now_fn = v8::Function::new(scope, perf_now).unwrap();
    let k = v8::String::new(scope, "now").unwrap();
    perf.set(scope, k.into(), now_fn.into());

    let mark_fn = v8::Function::new(scope, perf_mark).unwrap();
    let k = v8::String::new(scope, "mark").unwrap();
    perf.set(scope, k.into(), mark_fn.into());

    let measure_fn = v8::Function::new(scope, perf_measure).unwrap();
    let k = v8::String::new(scope, "measure").unwrap();
    perf.set(scope, k.into(), measure_fn.into());

    let get_entries_fn = v8::Function::new(scope, perf_get_entries).unwrap();
    let k = v8::String::new(scope, "getEntries").unwrap();
    perf.set(scope, k.into(), get_entries_fn.into());

    let get_entries_by_type_fn = v8::Function::new(scope, perf_get_entries_by_type).unwrap();
    let k = v8::String::new(scope, "getEntriesByType").unwrap();
    perf.set(scope, k.into(), get_entries_by_type_fn.into());

    let get_entries_by_name_fn = v8::Function::new(scope, perf_get_entries_by_name).unwrap();
    let k = v8::String::new(scope, "getEntriesByName").unwrap();
    perf.set(scope, k.into(), get_entries_by_name_fn.into());

    let clear_marks_fn = v8::Function::new(scope, perf_clear_marks).unwrap();
    let k = v8::String::new(scope, "clearMarks").unwrap();
    perf.set(scope, k.into(), clear_marks_fn.into());

    let clear_measures_fn = v8::Function::new(scope, perf_clear_measures).unwrap();
    let k = v8::String::new(scope, "clearMeasures").unwrap();
    perf.set(scope, k.into(), clear_measures_fn.into());

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

    // ─── Window dialog/messaging methods ────────────────────────────────
    // postMessage — no-op (SSR has no cross-origin messaging)
    let pm = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {
        log::trace!("window.postMessage() called (no-op in SSR)");
    }).unwrap();
    let key = v8::String::new(scope, "postMessage").unwrap();
    global.set(scope, key.into(), pm.into());

    // alert — no-op
    let alert_fn = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {
        log::trace!("window.alert() called (no-op in SSR)");
    }).unwrap();
    let key = v8::String::new(scope, "alert").unwrap();
    global.set(scope, key.into(), alert_fn.into());

    // confirm — returns false (SSR: no user interaction)
    let confirm_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        log::trace!("window.confirm() called (returns false in SSR)");
        rv.set(v8::Boolean::new(scope, false).into());
    }).unwrap();
    let key = v8::String::new(scope, "confirm").unwrap();
    global.set(scope, key.into(), confirm_fn.into());

    // prompt — returns null (SSR: no user interaction)
    let prompt_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        log::trace!("window.prompt() called (returns null in SSR)");
        rv.set(v8::null(scope).into());
    }).unwrap();
    let key = v8::String::new(scope, "prompt").unwrap();
    global.set(scope, key.into(), prompt_fn.into());

    // open — returns null (SSR: no popup windows)
    let open_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        log::trace!("window.open() called (returns null in SSR)");
        rv.set(v8::null(scope).into());
    }).unwrap();
    let key = v8::String::new(scope, "open").unwrap();
    global.set(scope, key.into(), open_fn.into());

    // close / focus / blur / print / stop — no-op
    let win_noop = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    for name in &["close", "focus", "blur", "print", "stop", "find"] {
        let key = v8::String::new(scope, name).unwrap();
        global.set(scope, key.into(), win_noop.into());
    }

    // ─── CSS global object ──────────────────────────────────────────────
    install_css_global(scope, global);

    // ─── FileList constructor ───────────────────────────────────────────
    install_filelist(scope, global);

    // ─── DOMException constructor ───────────────────────────────────────
    install_dom_exception(scope, global);

    // ─── EventTarget constructor ────────────────────────────────────────
    super::event_constructors::install_event_target(scope, global);

    // ─── ShadowRoot constructor ─────────────────────────────────────────
    super::shadow_root::install(scope, global);

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

    // history object — JS-based with real state tracking
    install_history(scope);

    // ─── Phase 7: Geometry, WebSocket, BroadcastChannel, document.fonts ──
    install_geometry_constructors(scope);
    install_websocket_constructor(scope);
    install_broadcast_channel(scope);
    install_document_fonts_and_state(scope);

    // performance.timeOrigin
    {
        let key = v8::String::new(scope, "performance").unwrap();
        if let Some(perf_val) = global.get(scope, key.into()) {
            if perf_val.is_object() {
                let perf = unsafe { v8::Local::<v8::Object>::cast_unchecked(perf_val) };
                let k = v8::String::new(scope, "timeOrigin").unwrap();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64() * 1000.0;
                let v = v8::Number::new(scope, now);
                perf.set(scope, k.into(), v.into());
            }
        }
    }

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

    // ─── Window on* event handler properties ──────────────────────────────
    {
        let null = v8::null(scope);
        for name in &[
            "onload", "onerror", "onresize", "onscroll", "onpopstate",
            "onhashchange", "onmessage", "onbeforeunload", "onunhandledrejection",
            "ononline", "onoffline", "onunload", "onpagehide", "onpageshow",
            "onfocus", "onblur", "onlanguagechange", "onstorage",
        ] {
            let k = v8::String::new(scope, name).unwrap();
            global.set(scope, k.into(), null.into());
        }
    }

    // ─── DOM constructors with spec-correct prototype hierarchy ────────────
    // Polyfills (ShadyDOM, webcomponents-sd.js, Polymer) check and patch
    // prototypes like Element.prototype, Node.prototype, CharacterData.prototype.
    // Each constructor must have the correct inheritance chain.
    {
        let templates = scope.get_slot::<crate::js::templates::DomTemplates>().unwrap();
        // Clone all FunctionTemplate globals before releasing the borrow
        let node_ft_g = templates.node_function.clone();
        let elem_ft_g = templates.element_function.clone();
        let doc_ft_g = templates.document_function.clone();
        let text_ft_g = templates.text_function.clone();
        let comment_ft_g = templates.comment_function.clone();
        let chardata_ft_g = templates.characterdata_function.clone();
        let html_elem_ft_g = templates.html_element_function.clone();
        let html_media_ft_g = templates.html_media_function.clone();
        let docfrag_ft_g = templates.doc_fragment_function.clone();
        let doctype_ft_g = templates.doctype_function.clone();
        let svg_ft_g = templates.svg_element_function.clone();

        // Register constructors with REAL prototypes from FunctionTemplates.
        // These give proper instanceof support and prototype chains.
        let real_ctors: Vec<(&str, &v8::Global<v8::FunctionTemplate>)> = vec![
            ("Node", &node_ft_g),
            ("Element", &elem_ft_g),
            ("Document", &doc_ft_g),
            ("Text", &text_ft_g),
            ("Comment", &comment_ft_g),
            ("CharacterData", &chardata_ft_g),
            ("HTMLElement", &html_elem_ft_g),
            ("HTMLMediaElement", &html_media_ft_g),
            ("DocumentFragment", &docfrag_ft_g),
            ("DocumentType", &doctype_ft_g),
            ("SVGElement", &svg_ft_g),
        ];
        for (name, ft_global) in &real_ctors {
            let ft = v8::Local::new(scope, *ft_global);
            let func = ft.get_function(scope).unwrap();
            let key = v8::String::new(scope, name).unwrap();
            global.set(scope, key.into(), func.into());
        }

        // ── Leaf HTML* types → share HTMLElement.prototype ──
        let html_proto = {
            let k = v8::String::new(scope, "HTMLElement").unwrap();
            let ctor = global.get(scope, k.into()).unwrap();
            let ctor_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(ctor) };
            let pk = v8::String::new(scope, "prototype").unwrap();
            ctor_obj.get(scope, pk.into()).unwrap()
        };
        // Inline leaf registration helper (avoids closure lifetime issues)
        macro_rules! register_leaf {
            ($scope:expr, $global:expr, $name:expr, $proto:expr) => {{
                let key = v8::String::new($scope, $name).unwrap();
                if $global.get($scope, key.into()).map(|v| v.is_undefined()).unwrap_or(true) {
                    let ctor = v8::Function::new($scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
                    let pk = v8::String::new($scope, "prototype").unwrap();
                    ctor.set($scope, pk.into(), $proto);
                    $global.set($scope, key.into(), ctor.into());
                }
            }};
        }

        for name in &[
            "HTMLDivElement", "HTMLSpanElement", "HTMLInputElement",
            "HTMLFormElement", "HTMLAnchorElement", "HTMLImageElement",
            "HTMLButtonElement", "HTMLSelectElement", "HTMLOptionElement",
            "HTMLTextAreaElement", "HTMLScriptElement", "HTMLStyleElement",
            "HTMLLinkElement", "HTMLMetaElement", "HTMLHeadElement",
            "HTMLBodyElement", "HTMLHtmlElement", "HTMLDocument",
            "HTMLTemplateElement", "HTMLCanvasElement", "HTMLIFrameElement",
            "HTMLTableElement", "HTMLTableRowElement", "HTMLTableCellElement",
            "HTMLParagraphElement", "HTMLPreElement", "HTMLUListElement",
            "HTMLOListElement", "HTMLLIElement", "HTMLLabelElement",
            "HTMLFieldSetElement", "HTMLLegendElement", "HTMLProgressElement",
            "HTMLDialogElement", "HTMLDetailsElement",
            "HTMLPictureElement", "HTMLSourceElement", "HTMLTrackElement",
            "HTMLUnknownElement",
        ] {
            register_leaf!(scope, global, name, html_proto);
        }

        // ── HTMLVideoElement, HTMLAudioElement → share HTMLMediaElement.prototype ──
        let media_proto = {
            let k = v8::String::new(scope, "HTMLMediaElement").unwrap();
            let ctor = global.get(scope, k.into()).unwrap();
            let ctor_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(ctor) };
            let pk = v8::String::new(scope, "prototype").unwrap();
            ctor_obj.get(scope, pk.into()).unwrap()
        };
        register_leaf!(scope, global, "HTMLVideoElement", media_proto);
        register_leaf!(scope, global, "HTMLAudioElement", media_proto);

        // ── CDATASection → share Text.prototype (CDATASection extends Text per spec) ──
        let text_proto = {
            let k = v8::String::new(scope, "Text").unwrap();
            let ctor = global.get(scope, k.into()).unwrap();
            let ctor_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(ctor) };
            let pk = v8::String::new(scope, "prototype").unwrap();
            ctor_obj.get(scope, pk.into()).unwrap()
        };
        register_leaf!(scope, global, "CDATASection", text_proto);

        // ── ProcessingInstruction → share CharacterData.prototype ──
        let chardata_proto = {
            let k = v8::String::new(scope, "CharacterData").unwrap();
            let ctor = global.get(scope, k.into()).unwrap();
            let ctor_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(ctor) };
            let pk = v8::String::new(scope, "prototype").unwrap();
            ctor_obj.get(scope, pk.into()).unwrap()
        };
        register_leaf!(scope, global, "ProcessingInstruction", chardata_proto);

        // ── Window constructor (special: prototype is the global itself) ──
        {
            let ctor = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
            let pk = v8::String::new(scope, "prototype").unwrap();
            ctor.set(scope, pk.into(), global.into());
            let key = v8::String::new(scope, "Window").unwrap();
            global.set(scope, key.into(), ctor.into());
        }

        // ── HTMLSlotElement → own prototype inheriting HTMLElement.prototype ──
        // with assignedNodes/assignedElements for polyfill detection
        let slot_source = r#"
        (function(g) {
            var proto = Object.create(g.HTMLElement.prototype);
            proto.assignedNodes = function() { return []; };
            proto.assignedElements = function() { return []; };
            proto.assign = function() {};
            proto.name = "";
            function HTMLSlotElement() { throw new TypeError("Illegal constructor"); }
            HTMLSlotElement.prototype = proto;
            proto.constructor = HTMLSlotElement;
            g.HTMLSlotElement = HTMLSlotElement;
        })(self)
        "#;
        run_js(scope, slot_source, "[blazeweb:HTMLSlotElement.prototype]");

        log::debug!("Installed DOM type hierarchy: Node → CharacterData/Element/Document/DocumentFragment/DocumentType, Element → HTMLElement → HTMLMediaElement, + 40 leaf types");
    }

    // ─── Custom element construction stack (must be AFTER HTMLElement is registered) ─
    super::custom_elements::install_construction_stack(scope, global);

    // ─── No-op constructors for non-DOM types ───────────────────────────

    let noop_ctor = v8::Function::new(scope, |_scope: &mut v8::HandleScope, _args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue| {}).unwrap();
    for name in &[
        "File", "FileReader",
        "Request", "Response",
        "AbortSignal", "XMLSerializer", "NodeList", "HTMLCollection",
        "DOMTokenList", "CSSStyleDeclaration", "NamedNodeMap",
        "NodeFilter", "TreeWalker", "Range", "Selection",
        "ReportingObserver",
        "Proxy", "WeakRef", "FinalizationRegistry",
        "BroadcastChannel", "MessagePort",
        "StyleSheet", "MediaQueryList",
    ] {
        let key = v8::String::new(scope, name).unwrap();
        // Only set if not already defined (functional constructors take priority)
        if global.get(scope, key.into()).map(|v| v.is_undefined()).unwrap_or(true) {
            global.set(scope, key.into(), noop_ctor.into());
        }
    }

    // NodeFilter constants — WHATWG DOM §6 NodeFilter interface
    // Must be a function (constructor) for `typeof NodeFilter === "function"` checks
    {
        let nf = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
        let nf_obj: v8::Local<v8::Object> = nf.into();
        let set_const = |scope: &mut v8::HandleScope, obj: v8::Local<v8::Object>, name: &str, val: u32| {
            let k = v8::String::new(scope, name).unwrap();
            let v = v8::Integer::new(scope, val as i32);
            obj.set(scope, k.into(), v.into());
        };
        set_const(scope, nf_obj, "FILTER_ACCEPT", 1);
        set_const(scope, nf_obj, "FILTER_REJECT", 2);
        set_const(scope, nf_obj, "FILTER_SKIP", 3);
        // SHOW_ALL is 0xFFFFFFFF — must use Number (not Integer) to preserve unsigned value
        let k = v8::String::new(scope, "SHOW_ALL").unwrap();
        let v = v8::Number::new(scope, 0xFFFFFFFF_u32 as f64);
        nf_obj.set(scope, k.into(), v.into());
        set_const(scope, nf_obj, "SHOW_ELEMENT", 0x1);
        set_const(scope, nf_obj, "SHOW_ATTRIBUTE", 0x2);
        set_const(scope, nf_obj, "SHOW_TEXT", 0x4);
        set_const(scope, nf_obj, "SHOW_CDATA_SECTION", 0x8);
        set_const(scope, nf_obj, "SHOW_PROCESSING_INSTRUCTION", 0x40);
        set_const(scope, nf_obj, "SHOW_COMMENT", 0x80);
        set_const(scope, nf_obj, "SHOW_DOCUMENT", 0x100);
        set_const(scope, nf_obj, "SHOW_DOCUMENT_TYPE", 0x200);
        set_const(scope, nf_obj, "SHOW_DOCUMENT_FRAGMENT", 0x400);
        let k = v8::String::new(scope, "NodeFilter").unwrap();
        global.set(scope, k.into(), nf_obj.into());
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

    // getPropertyValue(prop) — reads from this object's properties (inline styles + defaults)
    let gpv = v8::Function::new(scope, |scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        if args.length() < 1 { return; }
        let prop = args.get(0).to_rust_string_lossy(scope);
        let this = args.this();
        // Convert kebab-case to camelCase for property lookup
        let camel = css_prop_to_camel(&prop);
        let k = v8::String::new(scope, &camel).unwrap();
        if let Some(val) = this.get(scope, k.into()) {
            if !val.is_undefined() {
                rv.set(val);
                return;
            }
        }
        // Also try the original property name (kebab-case)
        let k = v8::String::new(scope, &prop).unwrap();
        if let Some(val) = this.get(scope, k.into()) {
            if !val.is_undefined() {
                rv.set(val);
                return;
            }
        }
        let empty = v8::String::new(scope, "").unwrap();
        rv.set(empty.into());
    }).unwrap();
    let k = v8::String::new(scope, "getPropertyValue").unwrap();
    obj.set(scope, k.into(), gpv.into());

    // setProperty / removeProperty — no-op
    let noop = v8::Function::new(scope, |_: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
    let k = v8::String::new(scope, "setProperty").unwrap();
    obj.set(scope, k.into(), noop.into());
    let k = v8::String::new(scope, "removeProperty").unwrap();
    obj.set(scope, k.into(), noop.into());

    // Set CSS initial values based on element type
    // Only set display if not already in inline styles
    if el_arg.is_object() {
        let el_obj = unsafe { v8::Local::<v8::Object>::cast_unchecked(el_arg) };
        if let Some(node_id) = crate::js::templates::unwrap_node_id(scope, el_obj) {
            let arena = arena_ref(scope);
            if let crate::dom::node::NodeData::Element(data) = &arena.nodes[node_id].data {
                let tag = &*data.name.local;
                let display = css_initial_display(tag);
                let k = v8::String::new(scope, "display").unwrap();
                if obj.get(scope, k.into()).map(|v| v.is_undefined()).unwrap_or(true) {
                    let v = v8::String::new(scope, display).unwrap();
                    obj.set(scope, k.into(), v.into());
                }
            }
        }
    }

    // Set common defaults for properties not already set
    let empty = v8::String::new(scope, "").unwrap();
    let common_props = ["visibility", "position", "overflow", "color",
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
    // Ensure display is set even if element wasn't found
    let k = v8::String::new(scope, "display").unwrap();
    if obj.get(scope, k.into()).map(|v| v.is_undefined()).unwrap_or(true) {
        obj.set(scope, k.into(), empty.into());
    }

    // length = 0
    let k = v8::String::new(scope, "length").unwrap();
    let val = v8::Integer::new(scope, 0);
    obj.set(scope, k.into(), val.into());

    rv.set(obj.into());
}

/// Convert CSS kebab-case property name to camelCase (e.g. "background-color" -> "backgroundColor").
fn css_prop_to_camel(prop: &str) -> String {
    let mut result = String::with_capacity(prop.len());
    let mut next_upper = false;
    for ch in prop.chars() {
        if ch == '-' {
            next_upper = true;
        } else if next_upper {
            result.push(ch.to_ascii_uppercase());
            next_upper = false;
        } else {
            result.push(ch);
        }
    }
    result
}

/// Return the CSS initial display value for a given HTML tag name.
/// Matches the UA stylesheet (ua.css) and WHATWG rendering spec §15.3.
fn css_initial_display(tag: &str) -> &'static str {
    match tag {
        // Block-level elements
        "html" | "body" | "div" | "section" | "nav" | "article" | "aside"
        | "header" | "footer" | "main" | "address" | "blockquote" | "center"
        | "details" | "dialog" | "dd" | "dl" | "dt" | "fieldset" | "figcaption"
        | "figure" | "form" | "hr" | "legend" | "listing" | "menu" | "ol"
        | "ul" | "p" | "pre" | "plaintext" | "search" | "summary" | "xmp"
        | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "block",
        // List items
        "li" => "list-item",
        // Table elements
        "table" => "table",
        "tr" => "table-row",
        "td" | "th" => "table-cell",
        "thead" => "table-header-group",
        "tbody" => "table-row-group",
        "tfoot" => "table-footer-group",
        "col" => "table-column",
        "colgroup" => "table-column-group",
        "caption" => "table-caption",
        // Hidden elements
        "head" | "link" | "meta" | "title" | "style" | "script" | "noscript"
        | "template" | "area" | "base" | "basefont" | "datalist" | "param"
        | "source" | "track" => "none",
        // Inline-block elements
        "img" | "svg" | "video" | "canvas" | "audio" | "iframe" | "embed"
        | "object" | "input" | "textarea" | "select" | "button" => "inline-block",
        // Default: inline
        _ => "inline",
    }
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

// ─── performance.* functions ─────────────────────────────────────────────────

/// Monotonic time counter for performance.now() — starts at 0 for each render.
fn perf_now(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    // In SSR context, return 0.0 (no real time origin).
    // Sites use this for relative timing, so 0 is fine.
    rv.set(v8::Number::new(scope, 0.0).into());
}

fn perf_mark(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let name = args.get(0).to_rust_string_lossy(scope);
    let mut start_time = 0.0;

    // Check for options with startTime
    if args.length() > 1 && args.get(1).is_object() {
        let opts = unsafe { v8::Local::<v8::Object>::cast_unchecked(args.get(1)) };
        let k = v8::String::new(scope, "startTime").unwrap();
        if let Some(st) = opts.get(scope, k.into()) {
            if st.is_number() {
                start_time = st.number_value(scope).unwrap_or(0.0);
            }
        }
    }

    let entry = super::observers::PerformanceEntry {
        name: name.clone(),
        entry_type: "mark".to_string(),
        start_time,
        duration: 0.0,
    };

    // Add to PerformanceObserverState timeline
    if let Some(state) = scope.get_slot_mut::<super::observers::PerformanceObserverState>() {
        state.add_entry(entry.clone());
    }

    // Return the PerformanceMark entry object
    let obj = v8::Object::new(scope);
    let k = v8::String::new(scope, "name").unwrap();
    let v = v8::String::new(scope, &entry.name).unwrap();
    obj.set(scope, k.into(), v.into());
    let k = v8::String::new(scope, "entryType").unwrap();
    let v = v8::String::new(scope, "mark").unwrap();
    obj.set(scope, k.into(), v.into());
    let k = v8::String::new(scope, "startTime").unwrap();
    let v = v8::Number::new(scope, entry.start_time);
    obj.set(scope, k.into(), v.into());
    let k = v8::String::new(scope, "duration").unwrap();
    let v = v8::Number::new(scope, 0.0);
    obj.set(scope, k.into(), v.into());
    rv.set(obj.into());
}

fn perf_measure(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let name = args.get(0).to_rust_string_lossy(scope);
    let mut start_time = 0.0;
    let mut end_time = 0.0;

    if args.length() > 1 {
        let arg1 = args.get(1);
        if arg1.is_string() {
            // measure(name, startMark) — look up mark time
            let start_mark = arg1.to_rust_string_lossy(scope);
            if let Some(state) = scope.get_slot::<super::observers::PerformanceObserverState>() {
                if let Some(t) = state.get_mark_time(&start_mark) {
                    start_time = t;
                }
            }
            if args.length() > 2 {
                let arg2 = args.get(2);
                if arg2.is_string() {
                    let end_mark = arg2.to_rust_string_lossy(scope);
                    if let Some(state) = scope.get_slot::<super::observers::PerformanceObserverState>() {
                        if let Some(t) = state.get_mark_time(&end_mark) {
                            end_time = t;
                        }
                    }
                }
            }
        } else if arg1.is_object() {
            // measure(name, options) — options has start/end/duration/detail
            let opts = unsafe { v8::Local::<v8::Object>::cast_unchecked(arg1) };
            let start_key = v8::String::new(scope, "start").unwrap();
            if let Some(sv) = opts.get(scope, start_key.into()) {
                if sv.is_number() {
                    start_time = sv.number_value(scope).unwrap_or(0.0);
                } else if sv.is_string() {
                    let mark_name = sv.to_rust_string_lossy(scope);
                    if let Some(state) = scope.get_slot::<super::observers::PerformanceObserverState>() {
                        if let Some(t) = state.get_mark_time(&mark_name) {
                            start_time = t;
                        }
                    }
                }
            }
            let end_key = v8::String::new(scope, "end").unwrap();
            if let Some(ev) = opts.get(scope, end_key.into()) {
                if ev.is_number() {
                    end_time = ev.number_value(scope).unwrap_or(0.0);
                } else if ev.is_string() {
                    let mark_name = ev.to_rust_string_lossy(scope);
                    if let Some(state) = scope.get_slot::<super::observers::PerformanceObserverState>() {
                        if let Some(t) = state.get_mark_time(&mark_name) {
                            end_time = t;
                        }
                    }
                }
            }
            let dur_key = v8::String::new(scope, "duration").unwrap();
            if let Some(dv) = opts.get(scope, dur_key.into()) {
                if dv.is_number() {
                    let dur = dv.number_value(scope).unwrap_or(0.0);
                    end_time = start_time + dur;
                }
            }
        }
    }

    let duration = end_time - start_time;
    let entry = super::observers::PerformanceEntry {
        name: name.clone(),
        entry_type: "measure".to_string(),
        start_time,
        duration,
    };

    if let Some(state) = scope.get_slot_mut::<super::observers::PerformanceObserverState>() {
        state.add_entry(entry.clone());
    }

    // Return the PerformanceMeasure entry object
    let obj = v8::Object::new(scope);
    let k = v8::String::new(scope, "name").unwrap();
    let v = v8::String::new(scope, &entry.name).unwrap();
    obj.set(scope, k.into(), v.into());
    let k = v8::String::new(scope, "entryType").unwrap();
    let v = v8::String::new(scope, "measure").unwrap();
    obj.set(scope, k.into(), v.into());
    let k = v8::String::new(scope, "startTime").unwrap();
    let v = v8::Number::new(scope, entry.start_time);
    obj.set(scope, k.into(), v.into());
    let k = v8::String::new(scope, "duration").unwrap();
    let v = v8::Number::new(scope, entry.duration);
    obj.set(scope, k.into(), v.into());
    rv.set(obj.into());
}

fn perf_get_entries(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let entries: Vec<super::observers::PerformanceEntry> = scope
        .get_slot::<super::observers::PerformanceObserverState>()
        .map(|s| s.get_timeline().to_vec())
        .unwrap_or_default();
    let arr = super::observers::build_performance_entries_array(scope, &entries);
    rv.set(arr.into());
}

fn perf_get_entries_by_type(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let type_str = args.get(0).to_rust_string_lossy(scope);
    let entries = scope
        .get_slot::<super::observers::PerformanceObserverState>()
        .map(|s| super::observers::PerformanceEntry::filter_by_type(s.get_timeline(), &type_str))
        .unwrap_or_default();
    let arr = super::observers::build_performance_entries_array(scope, &entries);
    rv.set(arr.into());
}

fn perf_get_entries_by_name(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let name_str = args.get(0).to_rust_string_lossy(scope);
    let type_filter = if args.length() > 1 && !args.get(1).is_undefined() {
        Some(args.get(1).to_rust_string_lossy(scope))
    } else {
        None
    };
    let entries = scope
        .get_slot::<super::observers::PerformanceObserverState>()
        .map(|s| super::observers::PerformanceEntry::filter_by_name(s.get_timeline(), &name_str, type_filter.as_deref()))
        .unwrap_or_default();
    let arr = super::observers::build_performance_entries_array(scope, &entries);
    rv.set(arr.into());
}

fn perf_clear_marks(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let name_filter = if args.length() > 0 && !args.get(0).is_undefined() {
        Some(args.get(0).to_rust_string_lossy(scope))
    } else {
        None
    };
    if let Some(state) = scope.get_slot_mut::<super::observers::PerformanceObserverState>() {
        state.clear_marks(name_filter.as_deref());
    }
}

fn perf_clear_measures(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let name_filter = if args.length() > 0 && !args.get(0).is_undefined() {
        Some(args.get(0).to_rust_string_lossy(scope))
    } else {
        None
    };
    if let Some(state) = scope.get_slot_mut::<super::observers::PerformanceObserverState>() {
        state.clear_measures(name_filter.as_deref());
    }
}

/// Install CSSStyleSheet constructor and document.adoptedStyleSheets.
fn install_cssstylesheet(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    let ctor = v8::Function::new(scope, |scope: &mut v8::HandleScope,
        _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let sheet = v8::Object::new(scope);
        // cssRules — empty array
        let k = v8::String::new(scope, "cssRules").unwrap();
        let arr = v8::Array::new(scope, 0);
        sheet.set(scope, k.into(), arr.into());
        // replaceSync(text) — accepts CSS text, no-op for SSR
        let replace_sync = v8::Function::new(scope, |_: &mut v8::HandleScope,
            _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {
            log::trace!("CSSStyleSheet.replaceSync() called");
        }).unwrap();
        let k = v8::String::new(scope, "replaceSync").unwrap();
        sheet.set(scope, k.into(), replace_sync.into());
        // replace(text) — returns resolved promise
        let replace = v8::Function::new(scope, |scope: &mut v8::HandleScope,
            _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
            let resolver = v8::PromiseResolver::new(scope).unwrap();
            let this = v8::undefined(scope);
            resolver.resolve(scope, this.into());
            rv.set(resolver.get_promise(scope).into());
        }).unwrap();
        let k = v8::String::new(scope, "replace").unwrap();
        sheet.set(scope, k.into(), replace.into());
        // insertRule / deleteRule — no-ops returning 0
        let insert_rule = v8::Function::new(scope, |scope: &mut v8::HandleScope,
            _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
            rv.set(v8::Integer::new(scope, 0).into());
        }).unwrap();
        let k = v8::String::new(scope, "insertRule").unwrap();
        sheet.set(scope, k.into(), insert_rule.into());
        let delete_rule = v8::Function::new(scope, |_: &mut v8::HandleScope,
            _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();
        let k = v8::String::new(scope, "deleteRule").unwrap();
        sheet.set(scope, k.into(), delete_rule.into());
        rv.set(sheet.into());
    }).unwrap();
    let k = v8::String::new(scope, "CSSStyleSheet").unwrap();
    global.set(scope, k.into(), ctor.into());

    // document.adoptedStyleSheets — empty array
    let doc_key = v8::String::new(scope, "document").unwrap();
    if let Some(doc_val) = global.get(scope, doc_key.into()) {
        if doc_val.is_object() {
            let doc = unsafe { v8::Local::<v8::Object>::cast_unchecked(doc_val) };
            let k = v8::String::new(scope, "adoptedStyleSheets").unwrap();
            let arr = v8::Array::new(scope, 0);
            doc.set(scope, k.into(), arr.into());
        }
    }
    log::debug!("Installed CSSStyleSheet constructor + document.adoptedStyleSheets");
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

// ─── CSS global object ──────────────────────────────────────────────────────

fn install_css_global(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    let css = v8::Object::new(scope);

    // CSS.supports(prop, val) or CSS.supports(conditionText) — returns false for SSR
    let supports_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        log::trace!("CSS.supports() called (returns false in SSR)");
        rv.set(v8::Boolean::new(scope, false).into());
    }).unwrap();
    let k = v8::String::new(scope, "supports").unwrap();
    css.set(scope, k.into(), supports_fn.into());

    // CSS.escape(ident) — per CSSOM spec, escapes special characters
    let escape_fn = v8::Function::new(scope, css_escape).unwrap();
    let k = v8::String::new(scope, "escape").unwrap();
    css.set(scope, k.into(), escape_fn.into());

    // CSS.highlights — empty Map-like
    let highlights = v8::Object::new(scope);
    let k = v8::String::new(scope, "highlights").unwrap();
    css.set(scope, k.into(), highlights.into());

    let key = v8::String::new(scope, "CSS").unwrap();
    global.set(scope, key.into(), css.into());
    log::debug!("Installed CSS global object (supports, escape, highlights)");
}

/// CSS.escape() per https://drafts.csswg.org/cssom/#the-css.escape()-method
fn css_escape(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let input = args.get(0).to_rust_string_lossy(scope);
    let mut result = String::with_capacity(input.len() * 2);
    for (i, ch) in input.chars().enumerate() {
        match ch {
            '\0' => result.push_str("\u{FFFD}"),
            '\x01'..='\x1F' | '\x7F' => {
                result.push('\\');
                result.push_str(&format!("{:x} ", ch as u32));
            }
            '-' if i == 0 && input.len() == 1 => {
                result.push('\\');
                result.push('-');
            }
            '0'..='9' if i == 0 => {
                result.push('\\');
                result.push_str(&format!("{:x} ", ch as u32));
            }
            '!' | '"' | '#' | '$' | '%' | '&' | '\'' | '(' | ')' | '*' | '+' | ',' | '.' | '/' | ':' | ';' | '<' | '=' | '>' | '?' | '@' | '[' | '\\' | ']' | '^' | '`' | '{' | '|' | '}' | '~' | ' ' => {
                result.push('\\');
                result.push(ch);
            }
            _ => result.push(ch),
        }
    }
    let v = v8::String::new(scope, &result).unwrap();
    rv.set(v.into());
}

// ─── FileList constructor ───────────────────────────────────────────────────

fn install_filelist(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    let ctor = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let obj = v8::Object::new(scope);
        let k = v8::String::new(scope, "length").unwrap();
        let v = v8::Integer::new(scope, 0);
        obj.set(scope, k.into(), v.into());
        let item_fn = v8::Function::new(scope, |scope: &mut v8::HandleScope, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
            rv.set(v8::null(scope).into());
        }).unwrap();
        let k = v8::String::new(scope, "item").unwrap();
        obj.set(scope, k.into(), item_fn.into());
        rv.set(obj.into());
    }).unwrap();
    let key = v8::String::new(scope, "FileList").unwrap();
    global.set(scope, key.into(), ctor.into());
    log::debug!("Installed FileList constructor");
}

// ─── DOMException constructor ───────────────────────────────────────────────

fn install_dom_exception(scope: &mut v8::HandleScope, global: v8::Local<v8::Object>) {
    let ctor = v8::Function::new(scope, |scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let obj = v8::Object::new(scope);
        // message (first arg, default "")
        let msg = if args.length() > 0 && !args.get(0).is_undefined() {
            args.get(0).to_rust_string_lossy(scope)
        } else {
            String::new()
        };
        let k = v8::String::new(scope, "message").unwrap();
        let v = v8::String::new(scope, &msg).unwrap();
        obj.set(scope, k.into(), v.into());

        // name (second arg, default "Error")
        let name = if args.length() > 1 && !args.get(1).is_undefined() {
            args.get(1).to_rust_string_lossy(scope)
        } else {
            "Error".to_string()
        };
        let k = v8::String::new(scope, "name").unwrap();
        let v = v8::String::new(scope, &name).unwrap();
        obj.set(scope, k.into(), v.into());

        // code — legacy error code based on name
        let code = match name.as_str() {
            "IndexSizeError" => 1,
            "HierarchyRequestError" => 3,
            "WrongDocumentError" => 4,
            "InvalidCharacterError" => 5,
            "NoModificationAllowedError" => 7,
            "NotFoundError" => 8,
            "NotSupportedError" => 9,
            "InvalidStateError" => 11,
            "SyntaxError" => 12,
            "InvalidModificationError" => 13,
            "NamespaceError" => 14,
            "InvalidAccessError" => 15,
            "TypeMismatchError" => 17,
            "SecurityError" => 18,
            "NetworkError" => 19,
            "AbortError" => 20,
            "URLMismatchError" => 21,
            "QuotaExceededError" => 22,
            "TimeoutError" => 23,
            "InvalidNodeTypeError" => 24,
            "DataCloneError" => 25,
            _ => 0,
        };
        let k = v8::String::new(scope, "code").unwrap();
        let v = v8::Integer::new(scope, code);
        obj.set(scope, k.into(), v.into());

        rv.set(obj.into());
    }).unwrap();
    let key = v8::String::new(scope, "DOMException").unwrap();
    global.set(scope, key.into(), ctor.into());
    log::debug!("Installed DOMException constructor");
}

// ─── History with real state tracking ───────────────────────────────────────

fn install_history(scope: &mut v8::HandleScope) {
    let source = r##"
    (function() {
        "use strict";
        var _state = null;
        var _length = 1;
        var _scrollRestoration = "auto";

        function updateLocation(url) {
            if (!url || typeof url !== "string") return;
            var L = self.location;
            if (!L) return;
            try {
                var hi = url.indexOf("#");
                var qi = url.indexOf("?");
                var pathname = qi !== -1 ? url.substring(0, qi) : (hi !== -1 ? url.substring(0, hi) : url);
                var search = "";
                var hash = "";
                if (qi !== -1) {
                    search = hi !== -1 ? url.substring(qi, hi) : url.substring(qi);
                }
                if (hi !== -1) hash = url.substring(hi);
                L.pathname = pathname;
                L.search = search;
                L.hash = hash;
                if (L.origin && L.origin !== "null") {
                    L.href = L.origin + pathname + search + hash;
                } else {
                    L.href = pathname + search + hash;
                }
            } catch(e) {}
        }

        var history = {
            get state() { return _state; },
            get length() { return _length; },
            get scrollRestoration() { return _scrollRestoration; },
            set scrollRestoration(v) { _scrollRestoration = String(v); },
            pushState: function(state, title, url) {
                _state = (state === undefined) ? null : state;
                _length++;
                if (url) updateLocation(url);
            },
            replaceState: function(state, title, url) {
                _state = (state === undefined) ? null : state;
                if (url) updateLocation(url);
            },
            back: function() {},
            forward: function() {},
            go: function() {}
        };
        return history;
    })()
    "##;
    let source_str = v8::String::new(scope, source).unwrap();
    let name = v8::String::new(scope, "[blazeweb:history]").unwrap();
    let origin = v8::ScriptOrigin::new(
        scope, name.into(), 0, 0, false, -1, None, false, false, false, None,
    );
    if let Some(script) = v8::Script::compile(scope, source_str, Some(&origin)) {
        if let Some(result) = script.run(scope) {
            let key = v8::String::new(scope, "history").unwrap();
            let global = scope.get_current_context().global(scope);
            global.set(scope, key.into(), result);
            log::debug!("Installed history object (JS-defined, supports pushState/replaceState with state tracking)");
        }
    }
}

// ─── Phase 7: Geometry constructors ─────────────────────────────────────────

fn install_geometry_constructors(scope: &mut v8::HandleScope) {
    let source = r#"
    (function(g) {
        function DOMRect(x,y,w,h) {
            this.x = x||0; this.y = y||0; this.width = w||0; this.height = h||0;
            this.top = Math.min(this.y, this.y+this.height);
            this.right = Math.max(this.x, this.x+this.width);
            this.bottom = Math.max(this.y, this.y+this.height);
            this.left = Math.min(this.x, this.x+this.width);
        }
        DOMRect.fromRect = function(r) { r=r||{}; return new DOMRect(r.x,r.y,r.width,r.height); };
        g.DOMRect = DOMRect;
        g.DOMRectReadOnly = DOMRect;

        function DOMPoint(x,y,z,w) { this.x=x||0; this.y=y||0; this.z=z||0; this.w=w===undefined?1:w; }
        DOMPoint.fromPoint = function(p) { p=p||{}; return new DOMPoint(p.x,p.y,p.z,p.w); };
        g.DOMPoint = DOMPoint;
        g.DOMPointReadOnly = DOMPoint;

        function DOMMatrix(init) {
            this.a=1;this.b=0;this.c=0;this.d=1;this.e=0;this.f=0;
            this.m11=1;this.m12=0;this.m13=0;this.m14=0;
            this.m21=0;this.m22=1;this.m23=0;this.m24=0;
            this.m31=0;this.m32=0;this.m33=1;this.m34=0;
            this.m41=0;this.m42=0;this.m43=0;this.m44=1;
            this.is2D=true; this.isIdentity=true;
        }
        DOMMatrix.prototype.transformPoint = function(p) { return new DOMPoint(p&&p.x||0, p&&p.y||0); };
        DOMMatrix.prototype.multiply = function() { return new DOMMatrix(); };
        DOMMatrix.prototype.translate = function() { return new DOMMatrix(); };
        DOMMatrix.prototype.scale = function() { return new DOMMatrix(); };
        DOMMatrix.prototype.rotate = function() { return new DOMMatrix(); };
        DOMMatrix.prototype.inverse = function() { return new DOMMatrix(); };
        DOMMatrix.prototype.toString = function() { return "matrix(1, 0, 0, 1, 0, 0)"; };
        g.DOMMatrix = DOMMatrix;
        g.DOMMatrixReadOnly = DOMMatrix;

        function DOMQuad(p1,p2,p3,p4) {
            this.p1 = p1||new DOMPoint(); this.p2 = p2||new DOMPoint();
            this.p3 = p3||new DOMPoint(); this.p4 = p4||new DOMPoint();
        }
        DOMQuad.prototype.getBounds = function() { return new DOMRect(); };
        g.DOMQuad = DOMQuad;
    })(self)
    "#;
    run_js(scope, source, "[blazeweb:geometry]");
    log::debug!("Installed DOMRect/DOMPoint/DOMMatrix/DOMQuad constructors");
}

// ─── Phase 7: WebSocket constructor ─────────────────────────────────────────

fn install_websocket_constructor(scope: &mut v8::HandleScope) {
    let source = r#"
    (function(g) {
        function WebSocket(url, protocols) {
            this.url = url || "";
            this.readyState = 3;
            this.protocol = "";
            this.extensions = "";
            this.bufferedAmount = 0;
            this.binaryType = "blob";
            this.onopen = null;
            this.onclose = null;
            this.onerror = null;
            this.onmessage = null;
        }
        WebSocket.CONNECTING = 0;
        WebSocket.OPEN = 1;
        WebSocket.CLOSING = 2;
        WebSocket.CLOSED = 3;
        WebSocket.prototype.send = function() {};
        WebSocket.prototype.close = function() {};
        WebSocket.prototype.addEventListener = function() {};
        WebSocket.prototype.removeEventListener = function() {};
        g.WebSocket = WebSocket;
    })(self)
    "#;
    run_js(scope, source, "[blazeweb:WebSocket]");
    log::debug!("Installed WebSocket constructor with CONNECTING/OPEN/CLOSING/CLOSED constants");
}

// ─── Phase 7: BroadcastChannel constructor ──────────────────────────────────

fn install_broadcast_channel(scope: &mut v8::HandleScope) {
    let source = r#"
    (function(g) {
        function BroadcastChannel(name) {
            this.name = name || "";
            this.onmessage = null;
            this.onmessageerror = null;
        }
        BroadcastChannel.prototype.postMessage = function() {};
        BroadcastChannel.prototype.close = function() {};
        BroadcastChannel.prototype.addEventListener = function() {};
        BroadcastChannel.prototype.removeEventListener = function() {};
        g.BroadcastChannel = BroadcastChannel;
    })(self)
    "#;
    run_js(scope, source, "[blazeweb:BroadcastChannel]");
    log::debug!("Installed BroadcastChannel constructor");
}

// ─── Phase 7: document.fonts + document state ───────────────────────────────

fn install_document_fonts_and_state(scope: &mut v8::HandleScope) {
    let source = r#"
    (function() {
        var doc = self.document;
        if (!doc) return;

        // document.fonts (FontFaceSet stub)
        var fonts = {
            ready: Promise.resolve(),
            status: "loaded",
            size: 0,
            check: function() { return true; },
            load: function() { return Promise.resolve([]); },
            forEach: function() {},
            entries: function() { return [][Symbol.iterator](); },
            keys: function() { return [][Symbol.iterator](); },
            values: function() { return [][Symbol.iterator](); },
            has: function() { return false; },
            add: function() {},
            delete: function() { return false; },
            clear: function() {},
            addEventListener: function() {},
            removeEventListener: function() {}
        };
        Object.defineProperty(doc, "fonts", { value: fonts, writable: false, enumerable: true });

        // document.visibilityState
        Object.defineProperty(doc, "visibilityState", { value: "visible", writable: false, enumerable: true });
        Object.defineProperty(doc, "hidden", { value: false, writable: false, enumerable: true });

        // document.scrollingElement
        Object.defineProperty(doc, "scrollingElement", {
            get: function() { return doc.documentElement; },
            enumerable: true
        });

        // document.fullscreenElement / fullscreenEnabled
        Object.defineProperty(doc, "fullscreenElement", { value: null, writable: false, enumerable: true });
        Object.defineProperty(doc, "fullscreenEnabled", { value: false, writable: false, enumerable: true });
    })()
    "#;
    run_js(scope, source, "[blazeweb:document-state]");
    log::debug!("Installed document.fonts, visibilityState, hidden, scrollingElement, fullscreenElement");
}

/// Helper to compile and run a JS snippet in the current context.
pub(super) fn run_js(scope: &mut v8::HandleScope, source: &str, label: &str) {
    let source_str = v8::String::new(scope, source).unwrap();
    let name = v8::String::new(scope, label).unwrap();
    let origin = v8::ScriptOrigin::new(
        scope, name.into(), 0, 0, false, -1, None, false, false, false, None,
    );
    match v8::Script::compile(scope, source_str, Some(&origin)) {
        Some(script) => {
            if script.run(scope).is_none() {
                log::error!("JS runtime error in {}", label);
            }
        }
        None => {
            log::error!("JS compilation error in {}: failed to compile builtin script", label);
        }
    }
}
