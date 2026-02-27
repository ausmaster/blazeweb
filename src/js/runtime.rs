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
fn ensure_v8_initialized() {
    V8_INIT.call_once(|| {
        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();
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
}

/// Execute all scripts (inline + external) found in the parsed Arena.
///
/// External scripts are fetched in parallel via async HTTP, then all scripts
/// execute in document order. Returns collected errors (non-fatal — each
/// script/fetch error is captured but doesn't prevent subsequent scripts).
pub fn execute_scripts(arena: &mut Arena, base_url: Option<&str>) -> Result<Vec<String>, EngineError> {
    execute_scripts_inner(arena, base_url, None)
}

/// Execute scripts with an optional script cache for external fetches.
pub fn execute_scripts_with_cache(
    arena: &mut Arena,
    base_url: Option<&str>,
    cache_opts: Option<&crate::net::fetch::CacheOpts>,
) -> Result<Vec<String>, EngineError> {
    execute_scripts_inner(arena, base_url, cache_opts)
}

fn execute_scripts_inner(
    arena: &mut Arena,
    base_url: Option<&str>,
    cache_opts: Option<&crate::net::fetch::CacheOpts>,
) -> Result<Vec<String>, EngineError> {
    let mut scripts = extract_scripts(arena);
    if scripts.is_empty() {
        return Ok(vec![]); // Fast path: no V8 initialization needed
    }

    // Resolve external script URLs
    let mut errors = Vec::new();
    let externals: Vec<(usize, reqwest::Url)> = scripts
        .iter()
        .enumerate()
        .filter_map(|(i, s)| match &s.source {
            ScriptSource::External(src) => {
                match crate::net::fetch::resolve_url(src, base_url) {
                    Ok(url) => Some((i, url)),
                    Err(e) => {
                        errors.push(e.to_string());
                        None
                    }
                }
            }
            _ => None,
        })
        .collect();

    // Fetch external scripts (with or without cache)
    let fetched = match cache_opts {
        Some(opts) => crate::net::fetch::fetch_scripts_cached(externals, opts),
        None => crate::net::fetch::fetch_scripts(externals),
    };
    for (idx, result) in fetched {
        match result {
            Ok(text) => scripts[idx].source = ScriptSource::Inline(text),
            Err(e) => {
                errors.push(e.to_string());
                // Mark as empty so it's skipped during execution
                scripts[idx].source = ScriptSource::Inline(String::new());
            }
        }
    }

    ensure_v8_initialized();

    // Serialize isolate creation — V8 135's JSDispatchTable has a race
    // condition in TryAllocateEntryFromFreelist during concurrent Isolate::Init.
    // The lock is released immediately after creation; script execution is parallel.
    let isolate = &mut {
        let _guard = ISOLATE_LOCK.lock().unwrap();
        v8::Isolate::new(v8::CreateParams::default())
    };

    // Store arena pointer and wrapper cache in isolate slots
    let arena_ptr = arena as *mut Arena;
    isolate.set_slot(ArenaPtr(arena_ptr));
    isolate.set_slot(WrapperCache {
        map: HashMap::new(),
    });

    let handle_scope = &mut v8::HandleScope::new(isolate);

    // Create DOM templates (pre-context, in HandleScope<()>)
    let dom_templates = super::templates::create_dom_templates(handle_scope);
    handle_scope.set_slot(dom_templates);

    // Create global template and context
    let global_template = super::templates::create_global_template(handle_scope);
    let context = v8::Context::new(
        handle_scope,
        v8::ContextOptions {
            global_template: Some(global_template),
            ..Default::default()
        },
    );
    let scope = &mut v8::ContextScope::new(handle_scope, context);

    // Install document, window, console on globalThis
    super::bindings::window::install_globals(scope);

    // Execute each script in document order
    for (i, script) in scripts.iter().enumerate() {
        let source = match &script.source {
            ScriptSource::Inline(s) if !s.is_empty() => s.as_str(),
            _ => continue, // skip unfetched/empty scripts
        };
        if let Err(e) = execute_one_script(scope, source, &script.name, i) {
            errors.push(e.to_string());
        }
    }

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
                });
            } else {
                let text = collect_script_text(arena, node);
                if !text.is_empty() {
                    scripts.push(ScriptInfo {
                        source: ScriptSource::Inline(text),
                        name: format!("inline-script-{}", *inline_idx),
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
mod tests {
    use super::*;

    #[test]
    fn test_v8_initializes() {
        ensure_v8_initialized();
        ensure_v8_initialized(); // Second call should be fine
    }

    #[test]
    fn test_extract_scripts_empty() {
        let arena = crate::dom::treesink::parse("<html><body><p>No scripts</p></body></html>");
        let scripts = extract_scripts(&arena);
        assert!(scripts.is_empty());
    }

    #[test]
    fn test_extract_scripts_inline() {
        let arena = crate::dom::treesink::parse(
            "<html><body><script>var x = 1;</script></body></html>",
        );
        let scripts = extract_scripts(&arena);
        assert_eq!(scripts.len(), 1);
        assert!(matches!(&scripts[0].source, ScriptSource::Inline(s) if s == "var x = 1;"));
        assert_eq!(scripts[0].name, "inline-script-0");
    }

    #[test]
    fn test_extract_scripts_multiple() {
        let arena = crate::dom::treesink::parse(
            "<html><body>\
             <script>var a = 1;</script>\
             <script>var b = 2;</script>\
             <script>var c = 3;</script>\
             </body></html>",
        );
        let scripts = extract_scripts(&arena);
        assert_eq!(scripts.len(), 3);
        assert!(matches!(&scripts[0].source, ScriptSource::Inline(s) if s == "var a = 1;"));
        assert!(matches!(&scripts[1].source, ScriptSource::Inline(s) if s == "var b = 2;"));
        assert!(matches!(&scripts[2].source, ScriptSource::Inline(s) if s == "var c = 3;"));
    }

    #[test]
    fn test_extract_scripts_skips_non_js_type() {
        let arena = crate::dom::treesink::parse(
            r#"<html><body><script type="application/json">{"key": "value"}</script></body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert!(scripts.is_empty());
    }

    #[test]
    fn test_extract_scripts_skips_type_module() {
        let arena = crate::dom::treesink::parse(
            r#"<html><body><script type="module">import x from './x';</script></body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert!(scripts.is_empty());
    }

    #[test]
    fn test_extract_scripts_external() {
        let arena = crate::dom::treesink::parse(
            r#"<html><body><script src="app.js"></script></body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert_eq!(scripts.len(), 1);
        assert!(matches!(&scripts[0].source, ScriptSource::External(s) if s == "app.js"));
    }

    #[test]
    fn test_extract_scripts_mixed_order() {
        let arena = crate::dom::treesink::parse(
            r#"<html><body>
            <script>var a = 1;</script>
            <script src="lib.js"></script>
            <script>var b = 2;</script>
            </body></html>"#,
        );
        let scripts = extract_scripts(&arena);
        assert_eq!(scripts.len(), 3);
        assert!(matches!(&scripts[0].source, ScriptSource::Inline(s) if s == "var a = 1;"));
        assert!(matches!(&scripts[1].source, ScriptSource::External(s) if s == "lib.js"));
        assert!(matches!(&scripts[2].source, ScriptSource::Inline(s) if s == "var b = 2;"));
    }

    #[test]
    fn test_execute_noop_script() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><script>var x = 1;</script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None).unwrap();
        assert!(errors.is_empty());
    }

    #[test]
    fn test_execute_no_scripts_fast_path() {
        let mut arena =
            crate::dom::treesink::parse("<html><body><p>Hello</p></body></html>");
        let errors = execute_scripts(&mut arena, None).unwrap();
        assert!(errors.is_empty());
    }

    #[test]
    fn test_execute_syntax_error() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><script>function {</script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None).unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("SyntaxError"));
    }

    #[test]
    fn test_execute_runtime_error() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><script>undefined.foo</script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None).unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("TypeError"));
    }

    #[test]
    fn test_multiple_scripts_sequential() {
        // First script sets a global, second reads it
        let mut arena = crate::dom::treesink::parse(
            "<html><body>\
             <script>var shared = 42;</script>\
             <script>if (shared !== 42) throw new Error('not shared');</script>\
             </body></html>",
        );
        let errors = execute_scripts(&mut arena, None).unwrap();
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    #[test]
    fn test_e2e_create_and_set_textcontent() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"container\"></div><script>\
             var el = document.createElement('span');\
             el.textContent = 'dynamic';\
             document.getElementById('container').appendChild(el);\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains("<span>dynamic</span>"), "got: {}", html);
    }

    #[test]
    fn test_e2e_innerhtml_set() {
        let mut arena = crate::dom::treesink::parse(
            "<html><body><div id=\"target\">old</div><script>\
             document.getElementById('target').innerHTML = '<b>bold</b>';\
             </script></body></html>",
        );
        let errors = execute_scripts(&mut arena, None).unwrap();
        let html = crate::dom::serialize(&arena);
        assert!(errors.is_empty(), "errors: {:?}", errors);
        assert!(html.contains("<b>bold</b>"), "got: {}", html);
        assert!(!html.contains(">old<"), "got: {}", html);
    }
}
