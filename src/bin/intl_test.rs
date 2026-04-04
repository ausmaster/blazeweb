/// Standalone binary to test whether V8 Intl.DateTimeFormat hangs.
///
/// This runs as a regular Rust binary (not cdylib), so if it works
/// but the cdylib hangs, the issue is shared-library-specific.

use std::time::Instant;

fn run_js(scope: &mut v8::HandleScope, script: &str, label: &str, _timeout_secs: u64) -> bool {
    eprint!("[test] {}: ", label);
    let start = Instant::now();

    let source = v8::String::new(scope, script).unwrap();
    let compiled = v8::Script::compile(scope, source, None);

    match compiled {
        Some(script_obj) => {
            let result = script_obj.run(scope);
            let elapsed = start.elapsed();

            match result {
                Some(val) => {
                    let result_str = val.to_rust_string_lossy(scope);
                    eprintln!("OK ({:?}) => {}", elapsed, result_str);
                    true
                }
                None => {
                    eprintln!("EXCEPTION ({:?})", elapsed);
                    false
                }
            }
        }
        None => {
            eprintln!("COMPILE ERROR ({:?})", start.elapsed());
            false
        }
    }
}

fn main() {
    eprintln!("=== V8 Intl Diagnostic Test (standalone binary) ===\n");

    // Initialize V8
    eprintln!("[init] Initializing V8 platform...");
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();
    eprintln!("[init] V8 initialized OK\n");

    // Create isolate
    let params = v8::CreateParams::default()
        .heap_limits(0, 512 * 1024 * 1024);
    let isolate = &mut v8::Isolate::new(params);
    let handle_scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(handle_scope, Default::default());
    let scope = &mut v8::ContextScope::new(handle_scope, context);
    eprintln!("[init] Context created OK\n");

    // Phase 1: Quick tests that should work
    eprintln!("--- Phase 1: Known-working Intl APIs ---");
    run_js(scope, "typeof Intl", "typeof Intl", 5);
    run_js(scope, "Intl.getCanonicalLocales('en-US').join(',')", "getCanonicalLocales", 5);
    run_js(scope, "new Intl.Collator('en-US').compare('a', 'b')", "Collator", 5);
    run_js(scope, "new Intl.PluralRules('en-US').select(1)", "PluralRules", 5);
    run_js(scope, "new Intl.Segmenter('en-US').segment('hello').containing(0).segment", "Segmenter", 5);

    // Phase 2: ICU resource bundle diagnostics
    eprintln!("\n--- Phase 2: ICU Data Diagnostics ---");

    // Check what locales V8 thinks are available (this uses SkipResourceCheck which works)
    run_js(scope, r#"
        var locales = Intl.Collator.supportedLocalesOf(['en-US', 'de-DE', 'ja-JP', 'zh-CN']);
        locales.join(',')
    "#, "Collator.supportedLocalesOf", 5);

    // Check if basic locale operations work
    run_js(scope, r#"
        var c = new Intl.Collator('en-US');
        JSON.stringify(c.resolvedOptions())
    "#, "Collator.resolvedOptions", 5);

    // Phase 3: Try to narrow down exactly where NumberFormat hangs
    eprintln!("\n--- Phase 3: NumberFormat sub-steps ---");

    // First, test if we can even create the simplest NumberFormat
    // The hang is in AvailableLocales<CheckNumberElements> lazy init which calls
    // ures_open(nullptr, locale, &status) for each available locale.
    // This happens on FIRST USE of NumberFormat.

    // Try a micro-step: just reference NumberFormat constructor (no call)
    run_js(scope, "typeof Intl.NumberFormat", "typeof NumberFormat", 5);

    // Try supportedLocalesOf which triggers AvailableLocales population
    // THIS is what should hang because it triggers the lazy locale enumeration
    eprintln!("\n--- Phase 4: The hanging call ---");
    eprintln!("[info] About to call Intl.NumberFormat.supportedLocalesOf()...");
    eprintln!("[info] This is expected to hang (infinite loop in ICU ures_open)");
    eprintln!("[info] Process will be killed by timeout if it hangs\n");

    run_js(scope, r#"
        Intl.NumberFormat.supportedLocalesOf(['en-US']).join(',')
    "#, "NumberFormat.supportedLocalesOf", 30);

    eprintln!("\n=== All tests complete ===");
}
