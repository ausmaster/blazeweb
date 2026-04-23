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

/// Per-render module state held in an isolate slot.
///
/// Two concerns are split per HTML spec §8.1.3 "Module scripts":
///
/// - `modules`: URL→compiled module. **External modules only.** This is the
///   dedup cache used when an `import` specifier resolves to a URL — the
///   first fetch compiles, subsequent imports reuse. Inline module scripts
///   are **never** keyed here — the spec explicitly says inline modules are
///   not importable by URL ([HTML §8.1.3.10](https://html.spec.whatwg.org/multipage/webappapis.html#fetch-an-inline-module-script-graph)
///   — inline modules are added to the document's "list of scripts" but not
///   to the module map).
/// - `identity_to_url`: V8 identity hash → URL used for resolving **this
///   module's** import specifiers. For external modules this is the module's
///   own fetched URL; for inline modules it is the document's base URL. Used
///   by `resolve_callback` which receives the referrer `v8::Module` and
///   needs to know the base URL for its imports.
pub struct ModuleMap {
    pub modules: HashMap<String, v8::Global<v8::Module>>,
    pub identity_to_url: HashMap<i32, String>,
}

impl ModuleMap {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
            identity_to_url: HashMap::new(),
        }
    }

    /// Register an EXTERNAL module (one fetched via a `src` attr or `import`
    /// specifier). Stored in both `modules` (for URL dedup) and
    /// `identity_to_url` (for import resolution).
    pub fn insert_external(
        &mut self,
        url: String,
        module: v8::Global<v8::Module>,
        identity_hash: i32,
    ) {
        self.identity_to_url.insert(identity_hash, url.clone());
        self.modules.insert(url, module);
    }

    /// Register an INLINE module. Records only `identity_to_url` so
    /// `resolve_callback` can anchor this module's imports to the document
    /// base URL. **Not** placed in the URL→module map — per spec, inline
    /// modules are not importable by URL.
    pub fn insert_inline(&mut self, identity_hash: i32, base_url: String) {
        self.identity_to_url.insert(identity_hash, base_url);
    }

    /// Look up a module by URL. Matches only EXTERNAL modules.
    pub fn get(&self, url: &str) -> Option<&v8::Global<v8::Module>> {
        self.modules.get(url)
    }

    /// Look up the URL that a module's imports resolve against.
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
pub fn compile_module<'s, 'i>(
    scope: &mut v8::PinnedRef<'s, v8::HandleScope<'i>>,
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

    crate::try_catch!(let try_catch, scope);
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
    scope: &mut v8::PinnedRef<v8::HandleScope>,
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

    // Store in module map as an external module (URL→module for dedup).
    let global_module = v8::Global::new(scope, module);
    {
        let map = scope.get_slot_mut::<ModuleMap>().expect("ModuleMap not in isolate slot");
        map.insert_external(url.to_string(), global_module, identity_hash);
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
    v8::callback_scope!(unsafe scope, context);

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
            Some(g) => g.clone(),
            None => {
                log::warn!("[modules] resolve_callback: module not found in map: \"{}\"", resolved);
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
    let local = v8::Local::new(scope, &global_module);
    log::debug!(
        "[modules] resolve_callback: \"{}\" → status={:?} (referrer_hash={})",
        resolved, local.get_status(), referrer_hash,
    );
    Some(local)
}

// ── Import Meta Callback ──────────────────────────────────────────────────────

/// V8 HostInitializeImportMetaObjectCallback: sets `import.meta.url`.
unsafe extern "C" fn import_meta_callback(
    context: v8::Local<v8::Context>,
    module: v8::Local<v8::Module>,
    meta: v8::Local<v8::Object>,
) {
    // SAFETY: Standard V8 callback pattern.
    v8::callback_scope!(unsafe scope, context);
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
fn dynamic_import_callback<'s, 'i>(
    scope: &mut v8::PinnedRef<'s, v8::HandleScope<'i>>,
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

    // Drive the module through link+evaluate using the shared helper, which
    // respects V8's state machine (no double-instantiate, no double-evaluate)
    // and drives any pending TLA promise to settlement.
    {
        let module_local = v8::Local::new(scope, &module_global);
        log::debug!(
            "[modules] dynamic_import: pre-link url=\"{}\" status={:?}",
            resolved, module_local.get_status(),
        );
    }
    if let Err(e) = link_and_evaluate_module(scope, &module_global, &resolved) {
        let msg = v8::String::new(scope, &e.to_string()).unwrap();
        let exc = v8::Exception::error(scope, msg);
        resolver.reject(scope, exc);
        return Some(promise);
    }

    let module = v8::Local::new(scope, &module_global);

    // Propagate a post-evaluate error status (TLA rejection landed on the
    // module, or evaluation threw synchronously) to the import() promise.
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

/// Drive a pending TLA evaluation promise to settlement by interleaving
/// microtask checkpoints, fetch drains, and timer drains, bounded by
/// `max_rounds`. Returns once the promise is no longer Pending or the bound
/// is hit. Callers should re-check the promise state afterwards.
///
/// Spec note: browsers drive module evaluation via the event loop, awaiting
/// the `Module.Evaluate()` promise. blazeweb's render is synchronous, so we
/// manually pump the queues here — matches the observable effect of "the
/// caller awaits the promise" for modules whose async work is satisfied by
/// our supported async primitives (fetch, setTimeout). Real wall-clock time
/// does not pass; timer callbacks fire regardless of their delay.
fn drive_eval_promise_to_settlement(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    eval_promise: &v8::Global<v8::Promise>,
    max_rounds: usize,
) {
    for _ in 0..max_rounds {
        scope.perform_microtask_checkpoint();
        let local = v8::Local::new(scope, eval_promise);
        if local.state() != v8::PromiseState::Pending {
            return;
        }
        // Nudge async work: one fetch round + one timer round. Errors from
        // callback execution surface separately when the caller inspects
        // eval_promise state (rejection) or the document-level drain later.
        let _ = super::fetch::drain(scope, 1);
        let _ = super::timers::drain(scope, 1);
    }
}

/// Fetch the module's static import tree (recursively) and Link the module
/// graph. Stops short of Evaluate — per spec, browsers Link all top-level
/// modules on the page BEFORE any Evaluate runs, so shared deps are never
/// walked in `kEvaluating` state by a sibling top-level module's linker.
///
/// V8 DCHECK at `Module::PrepareInstantiate:240` (`DCHECK_NE(kEvaluating)`)
/// fires if our resolve_callback returns a module that's currently
/// evaluating — which can only happen if evaluation of a prior top-level
/// module has started and its TLA is pending.
fn fetch_and_link_module(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    global_module: &v8::Global<v8::Module>,
    module_url_for_imports: &str,
) -> Result<(), crate::error::EngineError> {
    crate::try_catch!(let tc, scope);

    // Fetch static deps recursively. Per HTML spec, the module's "fetch the
    // descendants" step loads all static imports before linking.
    let module = v8::Local::new(tc, global_module);
    if module.get_status() == v8::ModuleStatus::Uninstantiated {
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

                let resolved = match resolve_module_specifier(&specifier, module_url_for_imports) {
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
    }

    // Re-borrow — slot table may have grown during fetch.
    let module = v8::Local::new(tc, global_module);

    // Link. V8's `InstantiateModule` is only valid in "unlinked" state; its
    // internal DCHECK fires for kEvaluating/kLinking re-entry.
    let status = module.get_status();
    log::debug!(
        "[modules] fetch_and_link: url=\"{}\" status_before={:?}",
        module_url_for_imports, status,
    );
    match status {
        v8::ModuleStatus::Uninstantiated => {
            log::debug!(
                "[modules] fetch_and_link: calling instantiate_module on \"{}\"",
                module_url_for_imports,
            );
            if module.instantiate_module(tc, resolve_callback).is_none() {
                let msg = tc
                    .exception()
                    .map(|e| e.to_rust_string_lossy(tc))
                    .unwrap_or_else(|| "module instantiation failed".into());
                let stack = tc.stack_trace().map(|s| s.to_rust_string_lossy(tc));
                return Err(crate::error::EngineError::JsExecution { message: msg, stack });
            }
            let module = v8::Local::new(tc, global_module);
            log::debug!(
                "[modules] fetch_and_link: instantiate ok, url=\"{}\" status_after={:?}",
                module_url_for_imports, module.get_status(),
            );
            Ok(())
        }
        v8::ModuleStatus::Errored => {
            let exc = module.get_exception();
            let msg = exc.to_rust_string_lossy(tc);
            Err(crate::error::EngineError::JsExecution { message: msg, stack: None })
        }
        // Instantiating/Instantiated/Evaluating/Evaluated: linking is already
        // complete (or in progress, which would be a spec-violating re-entry
        // we don't expect on our single-threaded worker).
        _ => {
            log::debug!(
                "[modules] fetch_and_link: skipping link for \"{}\" (status={:?})",
                module_url_for_imports, status,
            );
            Ok(())
        }
    }
}

/// Evaluate a previously-linked module, driving any TLA promise to settlement.
fn evaluate_linked_module(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    global_module: &v8::Global<v8::Module>,
) -> Result<(), crate::error::EngineError> {
    crate::try_catch!(let tc, scope);
    let module = v8::Local::new(tc, global_module);
    let identity_hash = module.get_identity_hash().get();
    let status = module.get_status();
    let url_label = tc
        .get_slot::<ModuleMap>()
        .and_then(|m| m.url_for_identity(identity_hash).map(|s| s.to_string()))
        .unwrap_or_else(|| format!("hash={}", identity_hash));
    log::debug!("[modules] evaluate: url=\"{}\" status_before={:?}", url_label, status);

    match status {
        v8::ModuleStatus::Instantiated => {
            log::debug!(
                "[modules] evaluate: url=\"{}\" has_caught_before_evaluate={}",
                url_label, tc.has_caught(),
            );
            match module.evaluate(tc) {
                Some(result) => {
                    if let Ok(eval_promise) = v8::Local::<v8::Promise>::try_from(result) {
                        let state_before = eval_promise.state();
                        let promise_global = v8::Global::new(tc, eval_promise);
                        drive_eval_promise_to_settlement(tc, &promise_global, 16);
                        let eval_promise = v8::Local::new(tc, &promise_global);
                        let state_after = eval_promise.state();
                        log::debug!(
                            "[modules] evaluate: url=\"{}\" eval promise {:?} → {:?} has_caught={}",
                            url_label, state_before, state_after, tc.has_caught(),
                        );
                        if state_after == v8::PromiseState::Rejected {
                            let exc = eval_promise.result(tc);
                            let msg = exc.to_rust_string_lossy(tc);
                            return Err(crate::error::EngineError::JsExecution {
                                message: msg,
                                stack: None,
                            });
                        }
                    }
                    tc.perform_microtask_checkpoint();
                    let module = v8::Local::new(tc, global_module);
                    log::debug!(
                        "[modules] evaluate: url=\"{}\" status_after={:?} has_caught_final={}",
                        url_label, module.get_status(), tc.has_caught(),
                    );
                    Ok(())
                }
                None => {
                    let msg = tc
                        .exception()
                        .map(|e| e.to_rust_string_lossy(tc))
                        .unwrap_or_else(|| "module evaluation failed".into());
                    let stack = tc.stack_trace().map(|s| s.to_rust_string_lossy(tc));
                    Err(crate::error::EngineError::JsExecution { message: msg, stack })
                }
            }
        }
        v8::ModuleStatus::Evaluating | v8::ModuleStatus::Evaluated => {
            log::debug!(
                "[modules] evaluate: url=\"{}\" already {:?}, skipping",
                url_label, status,
            );
            tc.perform_microtask_checkpoint();
            Ok(())
        }
        v8::ModuleStatus::Errored => {
            let exc = module.get_exception();
            let msg = exc.to_rust_string_lossy(tc);
            Err(crate::error::EngineError::JsExecution { message: msg, stack: None })
        }
        other => Err(crate::error::EngineError::JsExecution {
            message: format!("module in unexpected status: {:?}", other),
            stack: None,
        }),
    }
}

/// Link and evaluate in one pass. Used for entry points where a single module
/// is being processed atomically (dynamic import callback). Top-level module
/// scripts on a page go through separate `fetch_and_link_module` +
/// `evaluate_linked_module` passes in `runtime.rs` so all linking completes
/// before any evaluation — otherwise a sibling module's TLA leaves its graph
/// in kEvaluating state, and our resolve_callback hands V8 that kEvaluating
/// module when a shared dep is walked, tripping V8's internal DCHECK.
fn link_and_evaluate_module(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    global_module: &v8::Global<v8::Module>,
    module_url_for_imports: &str,
) -> Result<(), crate::error::EngineError> {
    fetch_and_link_module(scope, global_module, module_url_for_imports)?;
    evaluate_linked_module(scope, global_module)
}

/// A prepared (compiled + registered) module script, ready for linking and
/// evaluation. `url_for_imports` is the URL against which this module's
/// `import` specifiers resolve (the module's own URL for external scripts,
/// the document base URL for inline scripts).
pub struct PreparedModule {
    pub module: v8::Global<v8::Module>,
    pub url_for_imports: String,
}

/// Prepare an inline `<script type="module">`: compile and register in the
/// ModuleMap. Per HTML spec, inline module scripts are NOT added to the
/// URL→module map (they are not importable by URL); only their
/// identity→base_url mapping is recorded so `resolve_callback` can anchor
/// their `import` specifiers against the document's base URL.
pub fn prepare_inline_module(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    source: &str,
    base_url: Option<&str>,
) -> Result<PreparedModule, crate::error::EngineError> {
    // Display name used for V8 stack traces / import.meta.url. Inline modules
    // have no URL per spec; browsers use the document URL for their
    // `import.meta.url`. We mirror that.
    let import_base = base_url.unwrap_or("about:blank").to_string();

    crate::try_catch!(let tc, scope);
    let module = match compile_module(tc, source, &import_base) {
        Ok(m) => m,
        Err(msg) => {
            return Err(crate::error::EngineError::JsExecution { message: msg, stack: None });
        }
    };
    let identity_hash = module.get_identity_hash().get();
    let g = v8::Global::new(tc, module);
    let map = tc.get_slot_mut::<ModuleMap>().expect("ModuleMap not in isolate slot");
    map.insert_inline(identity_hash, import_base.clone());
    Ok(PreparedModule { module: g, url_for_imports: import_base })
}

/// Prepare an external `<script type="module" src="...">`: compile (or reuse
/// a cached compile) and register in the ModuleMap keyed by `resolved_url`.
///
/// Reuses a previously-compiled module if this URL was already pulled in as a
/// dependency of another module script. Compiling the same source twice would
/// produce two distinct v8::Module objects — `resolve_callback` would hand
/// out the first while our own linking would operate on the second, creating
/// a graph mismatch.
pub fn prepare_external_module(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    source: &str,
    resolved_url: &str,
) -> Result<PreparedModule, crate::error::EngineError> {
    let cached = {
        let map = scope.get_slot::<ModuleMap>().expect("ModuleMap not in isolate slot");
        map.get(resolved_url).cloned()
    };

    let global_module = match cached {
        Some(g) => g,
        None => {
            crate::try_catch!(let tc, scope);
            let module = match compile_module(tc, source, resolved_url) {
                Ok(m) => m,
                Err(msg) => {
                    return Err(crate::error::EngineError::JsExecution { message: msg, stack: None });
                }
            };
            let identity_hash = module.get_identity_hash().get();
            let g = v8::Global::new(tc, module);
            let map = tc.get_slot_mut::<ModuleMap>().expect("ModuleMap not in isolate slot");
            map.insert_external(resolved_url.to_string(), g.clone(), identity_hash);
            g
        }
    };

    Ok(PreparedModule {
        module: global_module,
        url_for_imports: resolved_url.to_string(),
    })
}

/// Link a prepared module (fetch its static imports, then call V8 Link).
pub fn link_prepared_module(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    prepared: &PreparedModule,
) -> Result<(), crate::error::EngineError> {
    fetch_and_link_module(scope, &prepared.module, &prepared.url_for_imports)
}

/// Evaluate a prepared + linked module, driving TLA to settlement.
pub fn evaluate_prepared_module(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    prepared: &PreparedModule,
) -> Result<(), crate::error::EngineError> {
    evaluate_linked_module(scope, &prepared.module)
}
