/// V8 runtime lifecycle: initialization, script extraction, execution.
///
/// Creates a fresh V8 isolate per render() call. Scripts are extracted
/// from the parsed DOM tree and executed in document order.

use std::collections::HashMap;
use std::sync::{Mutex, Once};

use crate::dom::arena::{Arena, NodeId};
use crate::dom::node::NodeData;
use crate::error::EngineError;
use crate::js::templates::WrapperCache;

/// Raw pointer to Arena, stored in V8 isolate slot.
///
/// Safety: single-threaded, Arena outlives Isolate by stack construction.
pub struct ArenaPtr(pub *mut Arena);

static V8_INIT: Once = Once::new();

/// Serialize isolate creation to work around a race in V8 135's
/// JSDispatchTable freelist during concurrent Isolate::Init.
static ISOLATE_LOCK: Mutex<()> = Mutex::new(());

/// Ensure V8 platform is initialized exactly once per process.
///
/// ICU data is loaded via set_common_data_74() BEFORE V8::initialize(),
/// matching the pattern used by Deno (deno_core/runtime/setup.rs).
/// Uses deno_core_icudata v0.74.0 (ICU 74 data, matching v8 crate 135.1.1).
/// The order is critical:
/// 1. set_common_data_74() — provide ICU locale data
/// 2. V8::initialize_platform() — create V8 platform
/// 3. V8::initialize() — start V8 engine
fn ensure_v8_initialized() {
    V8_INIT.call_once(|| {
        // Load ICU data FIRST — before any V8 initialization.
        log::info!("loading ICU data ({} bytes)...", deno_core_icudata::ICU_DATA.len());
        match v8::icu::set_common_data_74(deno_core_icudata::ICU_DATA) {
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
    node_id: Option<NodeId>, // <script> element node for document.currentScript
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

fn execute_scripts_inner(
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
    let external_requests: Vec<(usize, crate::net::request::Request)> = scripts
        .iter()
        .enumerate()
        .filter_map(|(i, s)| match &s.source {
            ScriptSource::External(src) => {
                match crate::net::fetch::resolve_url(src, base_url) {
                    Ok(url) => Some((i, crate::net::request::Request::script(url))),
                    Err(e) => {
                        errors.push(e.to_string());
                        None
                    }
                }
            }
            _ => None,
        })
        .collect();

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

    ensure_v8_initialized();

    // Serialize isolate creation — V8 135's JSDispatchTable has a race
    // condition in TryAllocateEntryFromFreelist during concurrent Isolate::Init.
    // The lock is released immediately after creation; script execution is parallel.
    log::info!("[{}] acquiring isolate lock...", url_label);
    let isolate = &mut {
        let _guard = ISOLATE_LOCK.lock().unwrap();
        log::info!("[{}] creating V8 isolate (1GB heap)...", url_label);
        // 1 GB heap limit. Chrome uses ~80 MB for typical sites; 1 GB gives
        // ample headroom for heavy pages without unbounded growth.
        let params = v8::CreateParams::default()
            .heap_limits(0, 1024 * 1024 * 1024);
        let mut iso = v8::Isolate::new(params);
        log::info!("[{}] V8 isolate created, configuring...", url_label);
        iso.add_near_heap_limit_callback(near_heap_limit_callback, std::ptr::null_mut());
        iso.set_oom_error_handler(oom_error_handler);
        iso
    };
    log::info!("[{}] V8 isolate ready in {:?}", url_label, t0.elapsed());

    // Store arena pointer and wrapper cache in isolate slots
    let arena_ptr = arena as *mut Arena;
    isolate.set_slot(ArenaPtr(arena_ptr));
    isolate.set_slot(WrapperCache {
        map: HashMap::new(),
    });
    isolate.set_slot(crate::js::templates::ChildNodesCache {
        map: HashMap::new(),
    });
    isolate.set_slot(super::timers::TimerQueue::new());
    isolate.set_slot(super::events::EventListenerMap::new());
    isolate.set_slot(super::bindings::storage::WebStorage::new());
    isolate.set_slot(super::bindings::location::BaseUrl(base_url.map(|s| s.to_string())));
    isolate.set_slot(super::bindings::document::DocumentCookie(String::new()));
    isolate.set_slot(super::bindings::document::CurrentScriptId(None));
    isolate.set_slot(super::fetch::FetchQueue::new());
    isolate.set_slot(super::mutation_observer::MutationObserverState::new());
    isolate.set_slot(fetch_context.clone());

    log::info!("[{}] creating handle scope...", url_label);
    let handle_scope = &mut v8::HandleScope::new(isolate);

    // Create DOM templates (pre-context, in HandleScope<()>)
    log::info!("[{}] creating DOM templates...", url_label);
    let dom_templates = super::templates::create_dom_templates(handle_scope);
    handle_scope.set_slot(dom_templates);

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

    // Execute each script in document order
    let exec_start = std::time::Instant::now();
    let mut scripts_executed = 0usize;
    let mut scripts_errored = 0usize;
    for (i, script) in scripts.iter().enumerate() {
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
            "[{}] exec script {}/{} \"{}\" ({} bytes)",
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
    log::info!(
        "[{}] executed {}/{} scripts in {:?} ({} errors)",
        url_label, scripts_executed, scripts.len(), exec_start.elapsed(), scripts_errored,
    );

    // Post-script phase: fire DOMContentLoaded, then interleaved fetch+timer drain
    let dcl_errors = super::events::fire_dom_content_loaded(scope);
    scope.perform_microtask_checkpoint();
    errors.extend(dcl_errors);

    // Interleaved drain: fetch results may schedule timers, timer callbacks may fetch
    for drain_round in 0..10 {
        let fetch_errors = super::fetch::drain(scope, 1);
        errors.extend(fetch_errors);

        let timer_errors = super::timers::drain(scope, 1);
        errors.extend(timer_errors);

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

    log::info!(
        "[{}] total execution: {:?}, {} errors",
        url_label, t0.elapsed(), errors.len(),
    );
    Ok(errors)
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
            // Only execute classic scripts
            let type_attr = data.get_attribute("type").unwrap_or("");
            if !(type_attr.is_empty()
                || type_attr == "text/javascript"
                || type_attr == "application/javascript")
            {
                return; // Non-JS type (module, json, etc.)
            }
            if let Some(src) = data.get_attribute("src") {
                scripts.push(ScriptInfo {
                    source: ScriptSource::External(src.to_owned()),
                    name: format!("{}", src),
                    node_id: Some(node),
                });
            } else {
                let text = collect_script_text(arena, node);
                if !text.is_empty() {
                    scripts.push(ScriptInfo {
                        source: ScriptSource::Inline(text),
                        name: format!("inline-script-{}", *inline_idx),
                        node_id: Some(node),
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
    scope: &mut v8::HandleScope,
    source: &str,
    name: &str,
    _script_id: usize,
) -> Result<(), EngineError> {
    let try_catch = &mut v8::TryCatch::new(scope);

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
fn extract_js_error(try_catch: &mut v8::TryCatch<v8::HandleScope>) -> EngineError {
    let message = try_catch
        .exception()
        .map(|e| e.to_rust_string_lossy(try_catch))
        .unwrap_or_else(|| "unknown JS error".into());

    let stack = try_catch
        .stack_trace()
        .map(|s| s.to_rust_string_lossy(try_catch));

    EngineError::JsExecution { message, stack }
}


#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
