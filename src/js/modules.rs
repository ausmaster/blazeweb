/// ES Module support: compilation, resolution, fetching, instantiation, evaluation.
///
/// Implements the module loading pipeline per the HTML spec:
/// - Inline `<script type="module">` compiles and evaluates as modules
/// - Static `import` specifiers are resolved, fetched, compiled recursively
/// - Dynamic `import()` handled via V8's HostImportModuleDynamicallyCallback
/// - `import.meta.url` provided via HostInitializeImportMetaObjectCallback

use std::collections::HashMap;

use crate::net::fetch::FetchContext;
use crate::net::request::Request;

// ── Module Map (isolate slot) ─────────────────────────────────────────────────

/// Stores compiled modules keyed by URL, plus a reverse map for resolve lookups.
pub struct ModuleMap {
    /// URL → compiled V8 module
    pub modules: HashMap<String, v8::Global<v8::Module>>,
    /// Module identity hash → URL (for reverse lookups in resolve callback)
    pub identity_to_url: HashMap<i32, String>,
}

impl ModuleMap {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
            identity_to_url: HashMap::new(),
        }
    }

    /// Insert a compiled module into the map.
    pub fn insert(
        &mut self,
        url: String,
        module: v8::Global<v8::Module>,
        identity_hash: i32,
    ) {
        self.identity_to_url.insert(identity_hash, url.clone());
        self.modules.insert(url, module);
    }

    /// Look up a module by URL.
    pub fn get(&self, url: &str) -> Option<&v8::Global<v8::Module>> {
        self.modules.get(url)
    }

    /// Look up a module's URL by its identity hash.
    pub fn url_for_identity(&self, hash: i32) -> Option<&str> {
        self.identity_to_url.get(&hash).map(|s| s.as_str())
    }
}

// ── Module Specifier Resolution ───────────────────────────────────────────────

/// Resolve a module specifier per the HTML spec algorithm.
///
/// - Absolute URLs (`https://...`) → as-is
/// - Relative (`./`, `../`, `/`) → resolved against base_url
/// - Bare specifiers (`lodash`) → error (no import maps yet)
pub fn resolve_module_specifier(specifier: &str, base_url: &str) -> Result<String, String> {
    // Try as absolute URL first
    if let Ok(url) = url::Url::parse(specifier) {
        if url.scheme() == "http" || url.scheme() == "https" || url.scheme() == "file" || url.scheme() == "data" {
            log::debug!(
                "[modules] resolved specifier \"{}\" as absolute URL",
                specifier
            );
            return Ok(url.to_string());
        }
    }

    // Relative specifiers: must start with /, ./, or ../
    if specifier.starts_with('/')
        || specifier.starts_with("./")
        || specifier.starts_with("../")
    {
        let base = url::Url::parse(base_url).map_err(|e| {
            format!("invalid base URL \"{}\": {}", base_url, e)
        })?;
        let resolved = base.join(specifier).map_err(|e| {
            format!(
                "failed to resolve \"{}\" against \"{}\": {}",
                specifier, base_url, e
            )
        })?;
        log::debug!(
            "[modules] resolved relative specifier \"{}\" → \"{}\"",
            specifier,
            resolved
        );
        return Ok(resolved.to_string());
    }

    // Bare specifier — not supported without import maps
    Err(format!(
        "Cannot resolve bare module specifier \"{}\". Relative references must start with \"./\", \"../\", or \"/\".",
        specifier
    ))
}

// ── Module Compilation ────────────────────────────────────────────────────────

/// Compile source text as an ES module.
pub fn compile_module<'s>(
    scope: &mut v8::HandleScope<'s>,
    source: &str,
    name: &str,
) -> Result<v8::Local<'s, v8::Module>, String> {
    let source_str = v8::String::new(scope, source)
        .ok_or_else(|| "failed to create module source string".to_string())?;

    let resource_name: v8::Local<v8::Value> =
        v8::String::new(scope, name).unwrap().into();

    let origin = v8::ScriptOrigin::new(
        scope,
        resource_name,
        0,     // line offset
        0,     // column offset
        false, // is_shared_cross_origin
        -1,    // script_id
        None,  // source_map_url
        false, // is_opaque
        false, // is_wasm
        true,  // is_module ← THIS is the key difference from classic scripts
        None,  // host_defined_options
    );

    let mut v8_source = v8::script_compiler::Source::new(source_str, Some(&origin));

    let try_catch = &mut v8::TryCatch::new(scope);
    match v8::script_compiler::compile_module(try_catch, &mut v8_source) {
        Some(module) => {
            log::debug!("[modules] compiled module \"{}\" ({} bytes)", name, source.len());
            Ok(module)
        }
        None => {
            let msg = try_catch
                .exception()
                .map(|e| e.to_rust_string_lossy(try_catch))
                .unwrap_or_else(|| "unknown compilation error".into());
            log::warn!("[modules] compilation failed for \"{}\": {}", name, msg);
            Err(msg)
        }
    }
}

// ── Fetch + Compile + Resolve Dependencies ────────────────────────────────────

/// Fetch a module by URL, compile it, and recursively fetch its dependencies.
///
/// Returns the module's URL (after any redirects) for cache key purposes.
/// All compiled modules are stored in the ModuleMap isolate slot.
pub fn fetch_and_compile_module(
    scope: &mut v8::HandleScope,
    url: &str,
    fetch_context: &FetchContext,
    depth: usize,
) -> Result<String, String> {
    // Guard against excessive depth (likely circular)
    if depth > 50 {
        return Err(format!("module dependency depth exceeded 50 for \"{}\"", url));
    }

    // Check module map for already-compiled module
    {
        let map = scope.get_slot::<ModuleMap>().expect("ModuleMap not in isolate slot");
        if map.get(url).is_some() {
            log::debug!("[modules] cache hit for \"{}\"", url);
            return Ok(url.to_string());
        }
    }

    log::debug!("[modules] fetching \"{}\" (depth={})", url, depth);

    // Fetch the module source
    let parsed_url = url::Url::parse(url)
        .map_err(|e| format!("invalid module URL \"{}\": {}", url, e))?;
    let mut request = Request::script(parsed_url);
    let response = crate::net::fetch::fetch(&mut request, fetch_context);

    if response.is_network_error() || !response.ok() {
        let reason = if response.is_network_error() {
            response.status_text.clone()
        } else {
            format!("HTTP {}", response.status)
        };
        return Err(format!("failed to fetch module \"{}\": {}", url, reason));
    }

    let source = response.text();
    log::debug!("[modules] fetched \"{}\" ({} bytes)", url, source.len());

    // Compile the module
    let module = compile_module(scope, &source, url)?;
    let identity_hash = module.get_identity_hash().get();

    // Get dependencies before storing (need to resolve specifiers)
    let requests = module.get_module_requests();
    let request_count = requests.length();
    let mut dep_specifiers = Vec::new();
    for i in 0..request_count {
        let request: v8::Local<v8::ModuleRequest> = requests.get(scope, i).unwrap().try_into().unwrap();
        let specifier = request.get_specifier().to_rust_string_lossy(scope);
        dep_specifiers.push(specifier);
    }

    // Store in module map
    let global_module = v8::Global::new(scope, module);
    {
        let map = scope.get_slot_mut::<ModuleMap>().expect("ModuleMap not in isolate slot");
        map.insert(url.to_string(), global_module, identity_hash);
    }

    if !dep_specifiers.is_empty() {
        log::debug!(
            "[modules] \"{}\" has {} dependencies: {:?}",
            url, dep_specifiers.len(), dep_specifiers,
        );
    }

    // Recursively fetch and compile dependencies
    for specifier in dep_specifiers {
        let resolved = resolve_module_specifier(&specifier, url)?;
        fetch_and_compile_module(scope, &resolved, fetch_context, depth + 1)?;
    }

    log::info!(
        "[modules] loaded \"{}\" (depth={}, {} deps)",
        url, depth, request_count,
    );
    Ok(url.to_string())
}

// ── Resolve Callback ──────────────────────────────────────────────────────────

/// V8 ResolveModuleCallback: called during `instantiate_module()` for each
/// static import. All modules must already be in the ModuleMap.
fn resolve_callback<'a>(
    context: v8::Local<'a, v8::Context>,
    specifier: v8::Local<'a, v8::String>,
    _import_attributes: v8::Local<'a, v8::FixedArray>,
    referrer: v8::Local<'a, v8::Module>,
) -> Option<v8::Local<'a, v8::Module>> {
    // SAFETY: We're inside a V8 callback; creating a CallbackScope is the standard pattern.
    let scope = &mut unsafe { v8::CallbackScope::new(context) };

    let specifier_str = specifier.to_rust_string_lossy(scope);
    let referrer_hash = referrer.get_identity_hash().get();

    log::trace!(
        "[modules] resolve_callback: specifier=\"{}\", referrer_hash={}",
        specifier_str, referrer_hash,
    );

    // Look up the referrer's URL
    let referrer_url = {
        let map = scope.get_slot::<ModuleMap>()?;
        map.url_for_identity(referrer_hash)?.to_string()
    };

    // Resolve the specifier against the referrer's URL
    let resolved = match resolve_module_specifier(&specifier_str, &referrer_url) {
        Ok(url) => url,
        Err(e) => {
            log::warn!("[modules] resolve failed: {}", e);
            let msg = v8::String::new(scope, &e).unwrap();
            scope.throw_exception(msg.into());
            return None;
        }
    };

    // Look up the resolved module (must already be compiled)
    let global_module = {
        let map = scope.get_slot::<ModuleMap>()?;
        match map.get(&resolved) {
            Some(g) => {
                log::trace!("[modules] resolve hit: \"{}\"", resolved);
                g.clone()
            }
            None => {
                log::warn!("[modules] module not found in map: \"{}\"", resolved);
                let msg = v8::String::new(
                    scope,
                    &format!("Module not found: {}", resolved),
                )
                .unwrap();
                scope.throw_exception(msg.into());
                return None;
            }
        }
    };
    Some(v8::Local::new(scope, &global_module))
}

// ── Import Meta Callback ──────────────────────────────────────────────────────

/// V8 HostInitializeImportMetaObjectCallback: sets `import.meta.url`.
unsafe extern "C" fn import_meta_callback(
    context: v8::Local<v8::Context>,
    module: v8::Local<v8::Module>,
    meta: v8::Local<v8::Object>,
) {
    // SAFETY: Standard V8 callback pattern.
    let scope = &mut unsafe { v8::CallbackScope::new(context) };
    let identity_hash = module.get_identity_hash().get();

    let url_str = scope
        .get_slot::<ModuleMap>()
        .and_then(|map| map.url_for_identity(identity_hash).map(|s| s.to_string()))
        .unwrap_or_else(|| "about:blank".to_string());

    let key = v8::String::new(scope, "url").unwrap();
    let value = v8::String::new(scope, &url_str).unwrap();
    meta.create_data_property(scope, key.into(), value.into());

    log::trace!("[modules] import.meta.url = \"{}\"", url_str);
}

// ── Dynamic Import Callback ──────────────────────────────────────────────────

/// V8 HostImportModuleDynamicallyCallback: handles `import()` expressions.
///
/// Fetches, compiles, instantiates, and evaluates the module, then resolves
/// the returned Promise with the module namespace object.
fn dynamic_import_callback<'s>(
    scope: &mut v8::HandleScope<'s>,
    _host_defined_options: v8::Local<'s, v8::Data>,
    resource_name: v8::Local<'s, v8::Value>,
    specifier: v8::Local<'s, v8::String>,
    _import_attributes: v8::Local<'s, v8::FixedArray>,
) -> Option<v8::Local<'s, v8::Promise>> {
    let specifier_str = specifier.to_rust_string_lossy(scope);
    let referrer_str = resource_name.to_rust_string_lossy(scope);

    log::debug!(
        "[modules] dynamic import(\"{}\") from \"{}\"",
        specifier_str, referrer_str,
    );

    // Create a promise resolver
    let resolver = v8::PromiseResolver::new(scope)?;
    let promise = resolver.get_promise(scope);

    // Determine base URL for resolution
    let base_url = if referrer_str.starts_with("http://")
        || referrer_str.starts_with("https://")
        || referrer_str.starts_with("file://")
    {
        referrer_str.clone()
    } else {
        // Referrer is an inline script name — use the page base URL
        scope
            .get_slot::<super::bindings::location::BaseUrl>()
            .and_then(|bu| bu.0.clone())
            .unwrap_or_else(|| "about:blank".to_string())
    };

    // Resolve specifier
    let resolved = match resolve_module_specifier(&specifier_str, &base_url) {
        Ok(url) => url,
        Err(e) => {
            let msg = v8::String::new(scope, &e).unwrap();
            let exc = v8::Exception::type_error(scope, msg);
            resolver.reject(scope, exc);
            return Some(promise);
        }
    };

    // Get FetchContext from isolate slot
    let fetch_context = scope
        .get_slot::<FetchContext>()
        .cloned()
        .unwrap_or_else(|| FetchContext::new(None));

    // Fetch, compile, and register the module and its dependencies
    match fetch_and_compile_module(scope, &resolved, &fetch_context, 0) {
        Ok(_) => {}
        Err(e) => {
            let msg = v8::String::new(scope, &e).unwrap();
            let exc = v8::Exception::type_error(scope, msg);
            resolver.reject(scope, exc);
            return Some(promise);
        }
    }

    // Get the compiled module
    let module_global = {
        let map = scope.get_slot::<ModuleMap>().expect("ModuleMap missing");
        match map.get(&resolved) {
            Some(g) => g.clone(),
            None => {
                let msg = v8::String::new(scope, "module not found after fetch").unwrap();
                let exc = v8::Exception::type_error(scope, msg);
                resolver.reject(scope, exc);
                return Some(promise);
            }
        }
    };

    let module = v8::Local::new(scope, &module_global);

    // Instantiate if not already done
    if module.get_status() == v8::ModuleStatus::Uninstantiated {
        let tc = &mut v8::TryCatch::new(scope);
        if module.instantiate_module(tc, resolve_callback).is_none() {
            let exc = tc.exception().unwrap_or_else(|| {
                v8::String::new(tc, "module instantiation failed").unwrap().into()
            });
            let exc = v8::Global::new(tc, exc);
            let exc = v8::Local::new(tc, &exc);
            resolver.reject(tc, exc);
            return Some(v8::Local::new(tc, promise));
        }
    }

    // Evaluate if not already done
    if module.get_status() == v8::ModuleStatus::Instantiated {
        let tc = &mut v8::TryCatch::new(scope);
        match module.evaluate(tc) {
            Some(result) => {
                // Module evaluation returns a Promise for async modules
                if let Ok(eval_promise) = v8::Local::<v8::Promise>::try_from(result) {
                    // Drain microtasks to settle the evaluation promise
                    tc.perform_microtask_checkpoint();
                    if eval_promise.state() == v8::PromiseState::Rejected {
                        let exc = eval_promise.result(tc);
                        resolver.reject(tc, exc);
                        return Some(v8::Local::new(tc, promise));
                    }
                }
                tc.perform_microtask_checkpoint();
            }
            None => {
                let exc = tc.exception().unwrap_or_else(|| {
                    v8::String::new(tc, "module evaluation failed").unwrap().into()
                });
                let exc = v8::Global::new(tc, exc);
                let exc = v8::Local::new(tc, &exc);
                resolver.reject(tc, exc);
                return Some(v8::Local::new(tc, promise));
            }
        }
    }

    // Check for evaluation errors
    if module.get_status() == v8::ModuleStatus::Errored {
        let exc = module.get_exception();
        resolver.reject(scope, exc);
        return Some(promise);
    }

    // Resolve with the module namespace
    let namespace = module.get_module_namespace();
    resolver.resolve(scope, namespace);
    log::debug!("[modules] dynamic import(\"{}\") resolved", resolved);
    Some(promise)
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Register module-related callbacks on the isolate.
/// Must be called before creating the V8 context.
pub fn register_module_callbacks(isolate: &mut v8::Isolate) {
    isolate.set_host_import_module_dynamically_callback(dynamic_import_callback);
    isolate.set_host_initialize_import_meta_object_callback(import_meta_callback);
    log::debug!("[modules] registered module callbacks on isolate");
}

#[cfg(test)]
#[path = "modules_tests.rs"]
mod tests;

/// Execute an inline module script. Compiles as a module, instantiates, and evaluates.
///
/// Module scripts have their own scope (no global variable leaking) and run
/// in strict mode with `this` === undefined at top level.
pub fn execute_one_module(
    scope: &mut v8::HandleScope,
    source: &str,
    name: &str,
    base_url: Option<&str>,
) -> Result<(), crate::error::EngineError> {
    let tc = &mut v8::TryCatch::new(scope);

    // Compile as module
    let module = match compile_module(tc, source, name) {
        Ok(m) => m,
        Err(msg) => {
            return Err(crate::error::EngineError::JsExecution {
                message: msg,
                stack: None,
            });
        }
    };

    let identity_hash = module.get_identity_hash().get();

    // Use the module name as URL for inline modules, or base_url if available
    let module_url = if name.starts_with("http://") || name.starts_with("https://") {
        name.to_string()
    } else {
        base_url.unwrap_or("about:blank").to_string()
    };

    // Store in module map
    let global_module = v8::Global::new(tc, module);
    {
        let map = tc
            .get_slot_mut::<ModuleMap>()
            .expect("ModuleMap not in isolate slot");
        map.insert(module_url.clone(), global_module.clone(), identity_hash);
    }

    // Fetch dependencies (if any static imports)
    let module = v8::Local::new(tc, &global_module);
    let requests = module.get_module_requests();
    let request_count = requests.length();

    if request_count > 0 {
        let fetch_context = tc
            .get_slot::<FetchContext>()
            .cloned()
            .unwrap_or_else(|| FetchContext::new(None));

        for i in 0..request_count {
            let request: v8::Local<v8::ModuleRequest> =
                requests.get(tc, i).unwrap().try_into().unwrap();
            let specifier = request.get_specifier().to_rust_string_lossy(tc);

            let resolved = match resolve_module_specifier(&specifier, &module_url) {
                Ok(url) => url,
                Err(e) => {
                    return Err(crate::error::EngineError::JsExecution {
                        message: e,
                        stack: None,
                    });
                }
            };

            if let Err(e) = fetch_and_compile_module(tc, &resolved, &fetch_context, 0) {
                return Err(crate::error::EngineError::JsExecution {
                    message: e,
                    stack: None,
                });
            }
        }
    }

    // Re-obtain module local handle (in case scope changed)
    let module = v8::Local::new(tc, &global_module);

    // Instantiate
    if module.instantiate_module(tc, resolve_callback).is_none() {
        let msg = tc
            .exception()
            .map(|e| e.to_rust_string_lossy(tc))
            .unwrap_or_else(|| "module instantiation failed".into());
        let stack = tc.stack_trace().map(|s| s.to_rust_string_lossy(tc));
        return Err(crate::error::EngineError::JsExecution {
            message: msg,
            stack,
        });
    }

    // Evaluate
    match module.evaluate(tc) {
        Some(result) => {
            // Module evaluation may return a Promise (top-level await)
            if let Ok(eval_promise) = v8::Local::<v8::Promise>::try_from(result) {
                tc.perform_microtask_checkpoint();
                if eval_promise.state() == v8::PromiseState::Rejected {
                    let exc = eval_promise.result(tc);
                    let msg = exc.to_rust_string_lossy(tc);
                    return Err(crate::error::EngineError::JsExecution {
                        message: msg,
                        stack: None,
                    });
                }
            }
            tc.perform_microtask_checkpoint();
            Ok(())
        }
        None => {
            let msg = tc
                .exception()
                .map(|e| e.to_rust_string_lossy(tc))
                .unwrap_or_else(|| "module evaluation failed".into());
            let stack = tc.stack_trace().map(|s| s.to_rust_string_lossy(tc));
            Err(crate::error::EngineError::JsExecution {
                message: msg,
                stack,
            })
        }
    }
}
