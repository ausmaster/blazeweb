/// V8 runtime lifecycle: initialization, script extraction, execution.
///
/// Uses long-lived isolates: one `v8::OwnedIsolate` per executor thread in
/// the `JsPool`, created once at worker startup. Each render gets a fresh
/// `v8::Context` on the worker's isolate and runs scripts in document order.

use std::collections::HashMap;
use std::sync::Once;

use crate::dom::arena::{Arena, NodeId};
use crate::dom::node::NodeData;
use crate::error::EngineError;
use crate::js::templates::WrapperCache;

/// Raw pointer to Arena, stored in V8 isolate slot.
///
/// Safety: single-threaded, Arena outlives Isolate by stack construction.
pub struct ArenaPtr(pub *mut Arena);

static V8_INIT: Once = Once::new();

/// Ensure V8 platform is initialized exactly once per process.
///
/// Uses `deno_core_icudata` (ICU 77 data) to match the ICU version bundled
/// with the v8 147 crate. Must call `v8::icu::set_common_data_77()` BEFORE
/// `V8::initialize()` (Deno's pattern from `deno_core/runtime/setup.rs`).
///
/// Order is critical:
/// 1. `set_common_data_77()` — provide ICU locale data
/// 2. `V8::initialize_platform()` — create V8 platform
/// 3. `V8::initialize()` — start V8 engine
fn ensure_v8_initialized() {
    V8_INIT.call_once(|| {
        // Load ICU data FIRST — before any V8 initialization.
        log::info!("loading ICU data ({} bytes)...", deno_core_icudata::ICU_DATA.len());
        match v8::icu::set_common_data_77(deno_core_icudata::ICU_DATA) {
            Ok(()) => log::info!("ICU data loaded successfully"),
            Err(code) => log::error!("ICU data load failed with error code {}", code),
        }

        log::info!("initializing V8 platform");
        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();
        log::info!("V8 initialized");
    });
}

/// Source of a script: inline text or external URL to fetch.
#[derive(Debug)]
enum ScriptSource {
    Inline(String),
    External(String), // src attribute value
}

/// A script extracted from the DOM tree.
struct ScriptInfo {
    source: ScriptSource,
    name: String,
    /// For external `<script src="...">` scripts, the fully-resolved absolute
    /// URL after joining `src` with the document's base URL. None for inline
    /// scripts. Set when the script is fetched; used as the module's own URL
    /// for external module scripts.
    resolved_url: Option<String>,
    node_id: Option<NodeId>, // <script> element node for document.currentScript
    is_module: bool,         // <script type="module">
}

/// Execute all scripts (inline + external) found in the parsed Arena.
///
/// External scripts are fetched in parallel via the unified fetch pipeline,
/// then all scripts execute in document order. Returns collected errors
/// (non-fatal — each script/fetch error is captured but doesn't prevent
/// subsequent scripts).
///
/// The `context` provides shared cache/cookies for script fetching and
/// is stored in the V8 isolate slot so JS fetch() and XHR can use it too.
pub fn execute_scripts(
    arena: &mut Arena,
    base_url: Option<&str>,
    context: &crate::net::fetch::FetchContext,
) -> Result<Vec<String>, EngineError> {
    execute_scripts_inner(arena, base_url, context)
}

/// Near-heap-limit callback: allow expansion up to 4x the initial heap limit.
/// Each expansion adds 256 MB. Once 4x is reached, V8 will OOM.
extern "C" fn near_heap_limit_callback(
    _data: *mut std::ffi::c_void,
    current_heap_limit: usize,
    initial_heap_limit: usize,
) -> usize {
    let max_limit = initial_heap_limit.saturating_mul(4);
    if current_heap_limit < max_limit {
        let expansion = 256 * 1024 * 1024; // 256 MB per step
        let new_limit = std::cmp::min(current_heap_limit + expansion, max_limit);
        log::warn!(
            "V8 near heap limit: current={}MB, expanding to {}MB (max={}MB)",
            current_heap_limit / (1024 * 1024),
            new_limit / (1024 * 1024),
            max_limit / (1024 * 1024),
        );
        new_limit
    } else {
        log::error!(
            "V8 heap at max {}MB — OOM imminent",
            current_heap_limit / (1024 * 1024),
        );
        current_heap_limit // No more expansions — V8 will OOM
    }
}

/// OOM error handler: logs diagnostic info before V8 aborts the process.
/// Cannot prevent the crash — V8 calls std::abort() after this returns.
unsafe extern "C" fn oom_error_handler(
    location: *const i8,
    details: &v8::OomDetails,
) {
    let loc = if location.is_null() {
        "unknown"
    } else {
        unsafe { std::ffi::CStr::from_ptr(location) }
            .to_str()
            .unwrap_or("unknown")
    };
    log::error!(
        "V8 fatal OOM at {}: is_heap_oom={}",
        loc,
        details.is_heap_oom,
    );
    eprintln!(
        "blazeweb: V8 fatal OOM at {}: is_heap_oom={}",
        loc, details.is_heap_oom,
    );
}

/// Build a fully-configured V8 isolate ready to host renders.
///
/// Called ONCE per executor thread at worker startup. The returned isolate has:
/// - 1 GB heap with 4× near-limit expansion callback,
/// - OOM error handler installed,
/// - module loader callbacks registered,
/// - DOM templates installed in an isolate slot.
///
/// Per-render state slots are NOT initialized here — call
/// `reset_per_render_slots` at the start of each render.
///
/// Safe to call concurrently from multiple worker threads — v8 14.7 fixed the
/// JSDispatchTable race that plagued v8 135, so no serialization is needed.
pub fn create_isolate_for_worker() -> v8::OwnedIsolate {
    ensure_v8_initialized();

    log::info!("creating V8 isolate (1 GB heap)...");
    let t0 = std::time::Instant::now();
    let params = v8::CreateParams::default()
        .heap_limits(0, 1024 * 1024 * 1024);
    let mut isolate = v8::Isolate::new(params);
    log::debug!("V8 isolate created in {:?}, configuring...", t0.elapsed());

    isolate.add_near_heap_limit_callback(near_heap_limit_callback, std::ptr::null_mut());
    isolate.set_oom_error_handler(oom_error_handler);
    super::modules::register_module_callbacks(&mut isolate);

    log::debug!("installing DOM templates on isolate...");
    super::templates::install_dom_templates(&mut isolate);

    log::info!("V8 isolate ready in {:?}", t0.elapsed());
    isolate
}

/// Reset all per-render isolate slots to fresh state.
///
/// This is the **single source of truth** for per-render state initialization.
/// In the long-lived isolate model (one isolate per worker thread), this is
/// called at the start of every render. In the current per-render isolate
/// model, it's called once per isolate (which is the same thing).
///
/// **Adding a new per-render slot:** add the `set_slot` call HERE, not in any
/// binding's `install` function. The state-isolation tests in
/// `tests/test_state_isolation.py` will catch a missing reset by failing
/// when render 2 sees render 1's state.
///
/// `DomTemplates` is intentionally NOT reset — it's per-isolate, not per-render.
pub fn reset_per_render_slots(
    isolate: &mut v8::Isolate,
    arena_ptr: *mut Arena,
    base_url: Option<&str>,
    fetch_context: crate::net::fetch::FetchContext,
) {
    log::debug!(
        "resetting per-render isolate slots (arena={:p}, base_url={:?})",
        arena_ptr, base_url,
    );
    isolate.set_slot(ArenaPtr(arena_ptr));
    isolate.set_slot(WrapperCache { map: HashMap::new() });
    isolate.set_slot(crate::js::templates::ChildNodesCache { map: HashMap::new() });
    isolate.set_slot(super::timers::TimerQueue::new());
    isolate.set_slot(super::events::EventListenerMap::new());
    isolate.set_slot(super::bindings::storage::WebStorage::new());
    isolate.set_slot(super::bindings::location::BaseUrl(base_url.map(|s| s.to_string())));
    isolate.set_slot(super::bindings::document::DocumentCookie(String::new()));
    isolate.set_slot(super::bindings::document::CurrentScriptId(None));
    isolate.set_slot(super::fetch::FetchQueue::new());
    isolate.set_slot(super::mutation_observer::MutationObserverState::new());
    isolate.set_slot(super::bindings::observers::IntersectionObserverState::new());
    isolate.set_slot(super::bindings::observers::ResizeObserverState::new());
    isolate.set_slot(super::bindings::observers::PerformanceObserverState::new());
    isolate.set_slot(super::modules::ModuleMap::new());
    isolate.set_slot(super::bindings::custom_elements::CustomElementState {
        definitions: HashMap::new(),
    });
    isolate.set_slot(fetch_context);
}

fn execute_scripts_inner(
    arena: &mut Arena,
    base_url: Option<&str>,
    fetch_context: &crate::net::fetch::FetchContext,
) -> Result<Vec<String>, EngineError> {
    execute_scripts_via_pool(arena, base_url, fetch_context, None)
}

/// Public dispatch entry point.
///
/// `pool=None` routes through the process-global default pool. `pool=Some(p)`
/// uses the caller-supplied pool (e.g. a Python `Client`'s per-instance pool).
pub fn execute_scripts_via_pool(
    arena: &mut Arena,
    base_url: Option<&str>,
    fetch_context: &crate::net::fetch::FetchContext,
    pool: Option<&super::executor::JsPool>,
) -> Result<Vec<String>, EngineError> {
    // Fast path: pages with no scripts skip the executor pool entirely (no V8
    // dispatch overhead, no isolate work).
    let url_label = base_url.unwrap_or("<inline>");
    if !arena_has_scripts(arena) {
        log::debug!("[{}] no scripts, skipping V8", url_label);
        return Ok(vec![]);
    }

    let arena_ptr = arena as *mut Arena;
    match pool {
        Some(p) => p.execute(arena_ptr, base_url, fetch_context.clone()),
        None => super::executor::default_pool()
            .execute(arena_ptr, base_url, fetch_context.clone()),
    }
}

/// Worker-side render entry point. Called by the executor thread with its
/// long-lived isolate. Does extract + fetch + V8 work and returns errors.
pub(crate) fn run_one_render(
    isolate: &mut v8::Isolate,
    arena: &mut Arena,
    base_url: Option<&str>,
    fetch_context: &crate::net::fetch::FetchContext,
) -> Result<Vec<String>, EngineError> {
    let t0 = std::time::Instant::now();
    let url_label = base_url.unwrap_or("<inline>");
    let mut scripts = extract_scripts(arena);
    if scripts.is_empty() {
        log::debug!("[{}] no scripts, skipping V8", url_label);
        return Ok(vec![]); // Fast path: no V8 initialization needed
    }

    let inline_count = scripts.iter().filter(|s| matches!(s.source, ScriptSource::Inline(_))).count();
    let external_count = scripts.len() - inline_count;
    log::info!(
        "[{}] found {} scripts ({} inline, {} external)",
        url_label, scripts.len(), inline_count, external_count,
    );

    // Resolve external script URLs and build Request objects
    let mut errors = Vec::new();
    let mut external_requests: Vec<(usize, crate::net::request::Request)> = Vec::new();
    for i in 0..scripts.len() {
        if let ScriptSource::External(src) = &scripts[i].source {
            match crate::net::fetch::resolve_url(src, base_url) {
                Ok(url) => {
                    // Stash the resolved URL so later phases (notably module
                    // execution) use it as the module's own URL for the
                    // ModuleMap and for resolving this module's imports.
                    scripts[i].resolved_url = Some(url.as_str().to_string());
                    external_requests.push((i, crate::net::request::Request::script(url)));
                }
                Err(e) => errors.push(e.to_string()),
            }
        }
    }

    // Fetch external scripts in parallel via the unified pipeline (uses shared cache/cookies)
    let fetch_start = std::time::Instant::now();
    let fetched = crate::net::fetch::fetch_parallel(external_requests, fetch_context);
    let mut fetch_ok = 0usize;
    let mut fetch_err = 0usize;
    for (idx, response) in fetched {
        if response.is_network_error() || !response.ok() {
            let reason = if response.is_network_error() {
                response.status_text.clone()
            } else {
                format!("HTTP {}", response.status)
            };
            log::warn!("[{}] fetch failed {}: {}", url_label, scripts[idx].name, reason);
            fetch_err += 1;
            errors.push(format!("network error fetching {}: {}", scripts[idx].name, reason));
            scripts[idx].source = ScriptSource::Inline(String::new());
        } else {
            let text = response.text();
            log::debug!("[{}] fetched {} ({} bytes)", url_label, scripts[idx].name, text.len());
            fetch_ok += 1;
            scripts[idx].source = ScriptSource::Inline(text);
        }
    }
    if external_count > 0 {
        log::info!(
            "[{}] fetched {}/{} external scripts in {:?} ({} failed)",
            url_label, fetch_ok, external_count, fetch_start.elapsed(), fetch_err,
        );
    }

    // Per-render slots — single source of truth. The isolate is long-lived
    // and serves many renders; this overwrites prior slot values with fresh
    // empty containers. v8::Globals from prior contexts are dropped here.
    log::debug!("[{}] resetting per-render isolate slots...", url_label);
    let arena_ptr = arena as *mut Arena;
    reset_per_render_slots(isolate, arena_ptr, base_url, fetch_context.clone());

    log::debug!("[{}] creating handle scope...", url_label);
    {
    v8::scope!(let handle_scope, isolate);

    // Create global template and context
    log::info!("[{}] creating global template...", url_label);
    let global_template = super::templates::create_global_template(handle_scope);
    log::info!("[{}] creating V8 context...", url_label);
    let context = v8::Context::new(
        handle_scope,
        v8::ContextOptions {
            global_template: Some(global_template),
            ..Default::default()
        },
    );
    log::info!("[{}] creating context scope...", url_label);
    let scope = &mut v8::ContextScope::new(handle_scope, context);

    // Install document, window, console on globalThis
    log::info!("[{}] installing globals...", url_label);
    super::bindings::window::install_globals(scope);
    log::info!("[{}] setup complete, executing scripts...", url_label);

    // Split scripts: classic first, then modules (per HTML spec, modules are deferred)
    let classic_count = scripts.iter().filter(|s| !s.is_module).count();
    let module_count = scripts.iter().filter(|s| s.is_module).count();
    log::info!(
        "[{}] executing {} classic scripts, then {} module scripts",
        url_label, classic_count, module_count,
    );

    let exec_start = std::time::Instant::now();
    let mut scripts_executed = 0usize;
    let mut scripts_errored = 0usize;

    // Phase 1: Execute classic scripts in document order
    for (i, script) in scripts.iter().enumerate() {
        if script.is_module {
            continue; // Modules run in phase 2
        }
        let source = match &script.source {
            ScriptSource::Inline(s) if !s.is_empty() => s.as_str(),
            _ => continue, // skip unfetched/empty scripts
        };
        // Set document.currentScript before execution
        if let Some(cs) = scope.get_slot_mut::<super::bindings::document::CurrentScriptId>() {
            cs.0 = script.node_id;
        }
        let script_start = std::time::Instant::now();
        log::debug!(
            "[{}] exec classic {}/{} \"{}\" ({} bytes)",
            url_label, i + 1, scripts.len(), script.name,
            source.len(),
        );
        match execute_one_script(scope, source, &script.name, i) {
            Ok(()) => {
                scripts_executed += 1;
                log::debug!(
                    "[{}] script \"{}\" ok in {:?}",
                    url_label, script.name, script_start.elapsed(),
                );
            }
            Err(e) => {
                scripts_errored += 1;
                log::warn!(
                    "[{}] script \"{}\" error in {:?}: {}",
                    url_label, script.name, script_start.elapsed(), e,
                );
                errors.push(e.to_string());
            }
        }
        // Reset document.currentScript after execution
        if let Some(cs) = scope.get_slot_mut::<super::bindings::document::CurrentScriptId>() {
            cs.0 = None;
        }
    }

    // Phase 2: Module scripts (deferred per spec). Split into three sub-phases
    // matching browser behavior:
    //   2A. Prepare (compile + register in ModuleMap) every module script.
    //   2B. Link all prepared modules in document order.
    //   2C. Evaluate all linked modules in document order, driving TLA to
    //       settlement between each.
    //
    // Step 2B must finish before 2C starts: V8's `InstantiateModule` walks
    // the dep graph and calls `PrepareInstantiate` on each dep. If any dep is
    // currently kEvaluating (because a sibling top-level module's TLA is
    // still pending), V8's internal DCHECK fires
    // (`v8/src/objects/module.cc:240`: `DCHECK_NE(status, kEvaluating)`).
    // Linking before any evaluation guarantees no kEvaluating state can be
    // observed during linking.
    //
    // Per spec ([HTML §8.1.3 "Processing model for module scripts"](https://html.spec.whatwg.org/multipage/webappapis.html#processing-model-for-module-scripts))
    // this matches "run a module script" which links once and then evaluates.
    let mut prepared_modules: Vec<(usize, super::modules::PreparedModule)> =
        Vec::new();
    for (i, script) in scripts.iter().enumerate() {
        if !script.is_module {
            continue;
        }
        let source = match &script.source {
            ScriptSource::Inline(s) if !s.is_empty() => s.as_str(),
            _ => continue,
        };
        let prep_result = match &script.resolved_url {
            Some(resolved) => super::modules::prepare_external_module(scope, source, resolved),
            None => super::modules::prepare_inline_module(scope, source, base_url),
        };
        match prep_result {
            Ok(prepared) => prepared_modules.push((i, prepared)),
            Err(e) => {
                scripts_errored += 1;
                log::warn!("[{}] module prepare \"{}\": {}", url_label, script.name, e);
                errors.push(e.to_string());
            }
        }
    }

    // 2B. Link all (V8 `InstantiateModule`). All modules are kUninstantiated
    // before this sub-phase; none are kEvaluating.
    for (i, prepared) in &prepared_modules {
        if let Some(cs) = scope.get_slot_mut::<super::bindings::document::CurrentScriptId>() {
            cs.0 = None;
        }
        let script = &scripts[*i];
        if let Err(e) = super::modules::link_prepared_module(scope, prepared) {
            scripts_errored += 1;
            log::warn!("[{}] module link \"{}\": {}", url_label, script.name, e);
            errors.push(e.to_string());
        }
    }

    // 2C. Evaluate all in document order. TLA in a module may leave it in
    // kEvaluating state, but the next module's Evaluate tolerates this (V8's
    // InstantiateModule is the only operation that trips on kEvaluating deps,
    // and all instantiation is already done).
    for (i, prepared) in &prepared_modules {
        let script = &scripts[*i];
        let script_start = std::time::Instant::now();
        log::debug!(
            "[{}] eval module {}/{} \"{}\"",
            url_label, i + 1, scripts.len(), script.name,
        );
        if let Some(cs) = scope.get_slot_mut::<super::bindings::document::CurrentScriptId>() {
            cs.0 = None;
        }
        match super::modules::evaluate_prepared_module(scope, prepared) {
            Ok(()) => {
                scripts_executed += 1;
                log::debug!(
                    "[{}] module \"{}\" ok in {:?}",
                    url_label, script.name, script_start.elapsed(),
                );
            }
            Err(e) => {
                scripts_errored += 1;
                log::warn!(
                    "[{}] module \"{}\" error in {:?}: {}",
                    url_label, script.name, script_start.elapsed(), e,
                );
                errors.push(e.to_string());
            }
        }
        scope.perform_microtask_checkpoint();
    }

    log::info!(
        "[{}] executed {}/{} scripts in {:?} ({} errors, {} classic, {} modules)",
        url_label, scripts_executed, scripts.len(), exec_start.elapsed(),
        scripts_errored, classic_count, module_count,
    );

    // Post-script phase: fire DOMContentLoaded, then interleaved fetch+timer drain
    let dcl_errors = super::events::fire_dom_content_loaded(scope);
    scope.perform_microtask_checkpoint();
    errors.extend(dcl_errors);

    // Fire observer callbacks before drain loop
    // (observers are typically set up during script execution / DOMContentLoaded)
    let io_errors = super::bindings::observers::drain_intersection_observers(scope);
    errors.extend(io_errors);
    let ro_errors = super::bindings::observers::drain_resize_observers(scope);
    errors.extend(ro_errors);
    let po_errors = super::bindings::observers::drain_performance_observers(scope);
    errors.extend(po_errors);
    scope.perform_microtask_checkpoint();

    // Interleaved drain: fetch results may schedule timers, timer callbacks may fetch
    for drain_round in 0..10 {
        let fetch_errors = super::fetch::drain(scope, 1);
        errors.extend(fetch_errors);

        let timer_errors = super::timers::drain(scope, 1);
        errors.extend(timer_errors);

        // Fire any newly registered observers from timer/fetch callbacks
        let io_errors = super::bindings::observers::drain_intersection_observers(scope);
        errors.extend(io_errors);
        let ro_errors = super::bindings::observers::drain_resize_observers(scope);
        errors.extend(ro_errors);
        let po_errors = super::bindings::observers::drain_performance_observers(scope);
        errors.extend(po_errors);

        scope.perform_microtask_checkpoint();

        let no_fetches = scope
            .get_slot::<super::fetch::FetchQueue>()
            .map_or(true, |q| q.pending.is_empty());
        let no_timers = scope
            .get_slot::<super::timers::TimerQueue>()
            .map_or(true, |q| q.is_empty());
        if no_fetches && no_timers {
            log::trace!("[{}] drain complete after {} rounds", url_label, drain_round + 1);
            break;
        }
        log::trace!("[{}] drain round {}: fetches={}, timers={}", url_label, drain_round + 1, !no_fetches, !no_timers);
    }
    } // end of v8::scope! block — ScopeStorage drops here

    log::info!(
        "[{}] total execution: {:?}, {} errors",
        url_label, t0.elapsed(), errors.len(),
    );
    Ok(errors)
}

/// Quick scan to check if the arena contains any <script> elements.
/// Used to skip pool dispatch entirely on JS-free pages.
fn arena_has_scripts(arena: &Arena) -> bool {
    fn walk(arena: &Arena, node: NodeId) -> bool {
        if let Some(data) = arena.element_data(node) {
            if &*data.name.local == "script" {
                return true;
            }
        }
        let mut child = arena.nodes[node].first_child;
        while let Some(c) = child {
            if walk(arena, c) {
                return true;
            }
            child = arena.nodes[c].next_sibling;
        }
        false
    }
    walk(arena, arena.document)
}

/// Walk the DOM tree depth-first, extract <script> elements in document order.
fn extract_scripts(arena: &Arena) -> Vec<ScriptInfo> {
    let mut scripts = Vec::new();
    let mut inline_idx = 0;
    extract_scripts_recursive(arena, arena.document, &mut scripts, &mut inline_idx);
    scripts
}

fn extract_scripts_recursive(
    arena: &Arena,
    node: NodeId,
    scripts: &mut Vec<ScriptInfo>,
    inline_idx: &mut usize,
) {
    if let Some(data) = arena.element_data(node) {
        if &*data.name.local == "script" {
            if data.script_already_started {
                return; // Already executed
            }
            // Skip <script nomodule> — we support modules, so nomodule fallback is skipped
            if data.get_attribute("nomodule").is_some() {
                return;
            }
            let type_attr = data.get_attribute("type").unwrap_or("");
            let is_module = type_attr == "module";
            // Accept classic JS types and module type
            if !(type_attr.is_empty()
                || type_attr == "text/javascript"
                || type_attr == "application/javascript"
                || is_module)
            {
                return; // Non-JS type (json, importmap, etc.)
            }
            if let Some(src) = data.get_attribute("src") {
                scripts.push(ScriptInfo {
                    source: ScriptSource::External(src.to_owned()),
                    name: format!("{}", src),
                    resolved_url: None,
                    node_id: Some(node),
                    is_module,
                });
            } else {
                let text = collect_script_text(arena, node);
                if !text.is_empty() {
                    scripts.push(ScriptInfo {
                        source: ScriptSource::Inline(text),
                        name: format!("inline-script-{}", *inline_idx),
                        resolved_url: None,
                        node_id: Some(node),
                        is_module,
                    });
                    *inline_idx += 1;
                }
            }
            return; // Don't recurse into <script> children
        }
    }

    // Recurse into children (but skip template contents)
    for child in arena.children(node) {
        extract_scripts_recursive(arena, child, scripts, inline_idx);
    }
}

/// Concatenate all text node children of a <script> element.
fn collect_script_text(arena: &Arena, node: NodeId) -> String {
    let mut text = String::new();
    for child in arena.children(node) {
        if let NodeData::Text(s) = &arena.nodes[child].data {
            text.push_str(s);
        }
    }
    text
}

/// Compile and run a single script. Returns error on exception.
fn execute_one_script(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    source: &str,
    name: &str,
    _script_id: usize,
) -> Result<(), EngineError> {
    crate::try_catch!(let try_catch, scope);

    let source_str = v8::String::new(try_catch, source).ok_or_else(|| {
        EngineError::JsExecution {
            message: "failed to create script source string".into(),
            stack: None,
        }
    })?;

    let resource_name: v8::Local<v8::Value> =
        v8::String::new(try_catch, name).unwrap().into();

    let origin = v8::ScriptOrigin::new(
        try_catch,
        resource_name,
        0,     // line offset
        0,     // column offset
        false, // is_shared_cross_origin
        -1,    // script_id
        None,  // source_map_url
        false, // is_opaque
        false, // is_wasm
        false, // is_module
        None,  // host_defined_options
    );

    let script = match v8::Script::compile(try_catch, source_str, Some(&origin)) {
        Some(s) => s,
        None => return Err(extract_js_error(try_catch)),
    };

    match script.run(try_catch) {
        Some(_) => Ok(()),
        None => Err(extract_js_error(try_catch)),
    }
}

/// Extract error info from a TryCatch scope.
fn extract_js_error(
    try_catch: &mut v8::PinnedRef<v8::TryCatch<v8::HandleScope>>,
) -> EngineError {
    let message = try_catch
        .exception()
        .map(|e: v8::Local<v8::Value>| e.to_rust_string_lossy(try_catch))
        .unwrap_or_else(|| "unknown JS error".into());

    let stack = try_catch
        .stack_trace()
        .map(|s: v8::Local<v8::Value>| s.to_rust_string_lossy(try_catch));

    EngineError::JsExecution { message, stack }
}


#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
