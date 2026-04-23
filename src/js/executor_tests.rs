    use super::*;
    use crate::dom::treesink::parse;
    use crate::net::fetch::FetchContext;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// Helper: render via a pool, parsing HTML on the test thread.
    fn render_through_pool(pool: &JsPool, html: &str) -> Result<Vec<String>, EngineError> {
        let mut arena = parse(html);
        let arena_ptr = &mut arena as *mut Arena;
        pool.execute(arena_ptr, None, FetchContext::new(None))
    }

    // ─── Basic lifecycle ──────────────────────────────────────────────────

    #[test]
    fn test_pool_starts_and_drops_cleanly() {
        let pool = JsPool::new(1).expect("pool init");
        // Drop without sending any jobs — workers should shut down on
        // channel disconnect or Shutdown message.
        drop(pool);
    }

    #[test]
    fn test_pool_workers_min_one() {
        // Requesting zero workers is clamped to 1 (the min).
        let pool = JsPool::new(0).expect("pool init");
        // If a worker was actually spawned, this render works.
        let result = render_through_pool(&pool, "<html><body></body></html>");
        assert!(result.is_ok());
    }

    // ─── Render correctness ───────────────────────────────────────────────

    #[test]
    fn test_pool_executes_no_script_html() {
        let pool = JsPool::new(1).expect("pool init");
        // Fast path: no scripts, executor not invoked, returns immediately.
        let result = render_through_pool(&pool, "<html><body><p>hi</p></body></html>");
        assert_eq!(result.unwrap(), Vec::<String>::new());
    }

    #[test]
    fn test_pool_executes_simple_script() {
        let pool = JsPool::new(1).expect("pool init");
        let mut arena = parse(
            "<html><body><script>document.title='hello';</script></body></html>",
        );
        let arena_ptr = &mut arena as *mut Arena;
        let errors = pool
            .execute(arena_ptr, None, FetchContext::new(None))
            .expect("execute");
        assert_eq!(errors, Vec::<String>::new(), "render had errors: {:?}", errors);
        // The script should have set the title — verify by re-serializing.
        let html = crate::dom::serialize::serialize_document(&arena);
        assert!(html.contains("<title>hello</title>"), "title not set: {html}");
    }

    // ─── State isolation across renders on the same isolate ───────────────

    #[test]
    fn test_pool_state_does_not_leak_between_renders() {
        let pool = JsPool::new(1).expect("pool init");
        // Render 1: set window.__leak.
        let _ = render_through_pool(
            &pool,
            "<html><body><script>window.__leak = 42;</script></body></html>",
        );
        // Render 2: assert it's undefined (state leaked into title 'LEAKED').
        let mut arena = parse(
            "<html><body><script>\
             document.title = (typeof window.__leak === 'undefined') ? 'fresh' : 'LEAKED';\
             </script></body></html>",
        );
        let arena_ptr = &mut arena as *mut Arena;
        pool.execute(arena_ptr, None, FetchContext::new(None))
            .expect("render 2");
        let html = crate::dom::serialize::serialize_document(&arena);
        assert!(html.contains("<title>fresh</title>"), "state leaked: {html}");
    }

    #[test]
    fn test_pool_serial_renders_stable_under_load() {
        let pool = JsPool::new(1).expect("pool init");
        for i in 0..50 {
            let html = format!(
                "<html><body><script>document.title='r{}'</script></body></html>",
                i,
            );
            let mut arena = parse(&html);
            let arena_ptr = &mut arena as *mut Arena;
            let errors = pool
                .execute(arena_ptr, None, FetchContext::new(None))
                .unwrap_or_else(|e| panic!("render {} failed: {:?}", i, e));
            assert!(errors.is_empty(), "render {} errors: {:?}", i, errors);
            let serialized = crate::dom::serialize::serialize_document(&arena);
            assert!(
                serialized.contains(&format!("<title>r{}</title>", i)),
                "render {} title missing in: {}",
                i, serialized,
            );
        }
    }

    // ─── Multi-worker dispatch ────────────────────────────────────────────

    #[test]
    fn test_pool_multi_worker_handles_concurrent_renders() {
        // 4 worker threads, 16 concurrent renders from 16 caller threads.
        // All must complete successfully without crashing.
        let pool = Arc::new(JsPool::new(4).expect("pool init"));
        let mut handles = Vec::new();
        for i in 0..16 {
            let pool = Arc::clone(&pool);
            handles.push(std::thread::spawn(move || {
                let html = format!(
                    "<html><body><script>document.title='t{}'</script></body></html>",
                    i,
                );
                let mut arena = parse(&html);
                let arena_ptr = &mut arena as *mut Arena;
                let errors = pool
                    .execute(arena_ptr, None, FetchContext::new(None))
                    .expect("execute");
                let serialized = crate::dom::serialize::serialize_document(&arena);
                (i, errors, serialized)
            }));
        }
        for h in handles {
            let (i, errors, serialized) = h.join().expect("worker panic");
            assert!(errors.is_empty(), "render {} errors: {:?}", i, errors);
            assert!(
                serialized.contains(&format!("<title>t{}</title>", i)),
                "render {} title missing in: {}",
                i, serialized,
            );
        }
    }

    // ─── Drop behavior ────────────────────────────────────────────────────

    #[test]
    fn test_pool_drop_releases_threads_promptly() {
        // Pool with 4 workers should drop within a reasonable time after
        // the last job completes (no leaks, no hung threads).
        let pool = JsPool::new(4).expect("pool init");
        let _ = render_through_pool(&pool, "<html><body></body></html>");
        let t0 = Instant::now();
        drop(pool);
        let elapsed = t0.elapsed();
        assert!(
            elapsed < Duration::from_secs(2),
            "pool drop took too long: {:?}",
            elapsed,
        );
    }

    // ─── JS execution timeout ─────────────────────────────────────────────

    #[test]
    fn test_pool_terminates_infinite_loop() {
        // 500 ms timeout — runaway script should be terminated within that.
        let pool = JsPool::with_timeout(1, Duration::from_millis(500))
            .expect("pool init");
        let mut arena = parse(
            "<html><body><script>while(true){}</script></body></html>",
        );
        let arena_ptr = &mut arena as *mut Arena;
        let t0 = Instant::now();
        let result = pool.execute(arena_ptr, None, FetchContext::new(None));
        let elapsed = t0.elapsed();
        // Must complete (with error) within 2s, well past the 500ms timeout.
        // If the timeout doesn't fire, the test would hang forever.
        assert!(
            elapsed < Duration::from_secs(2),
            "timeout did not fire promptly: {:?}",
            elapsed,
        );
        // The pool returns Ok(errors) — at least one error reflects termination.
        match result {
            Ok(errors) => {
                assert!(
                    !errors.is_empty(),
                    "expected timeout error, got empty errors after {:?}",
                    elapsed,
                );
            }
            Err(e) => {
                // Acceptable: top-level EngineError surfacing the timeout.
                let msg = format!("{:?}", e);
                assert!(
                    msg.contains("timeout") || msg.contains("terminate"),
                    "unexpected error type: {}",
                    msg,
                );
            }
        }
    }

    #[test]
    fn test_pool_isolate_recovers_after_timeout() {
        // After a timeout-killed render, the SAME isolate must serve the next
        // render correctly (no poisoned state).
        let pool = JsPool::with_timeout(1, Duration::from_millis(500))
            .expect("pool init");
        // Render 1: runaway, gets killed.
        {
            let mut arena = parse(
                "<html><body><script>while(true){}</script></body></html>",
            );
            let arena_ptr = &mut arena as *mut Arena;
            let _ = pool.execute(arena_ptr, None, FetchContext::new(None));
        }
        // Render 2: must succeed normally on the recovered isolate.
        let mut arena = parse(
            "<html><body><script>document.title='ok';</script></body></html>",
        );
        let arena_ptr = &mut arena as *mut Arena;
        let errors = pool
            .execute(arena_ptr, None, FetchContext::new(None))
            .expect("render 2 failed");
        assert!(errors.is_empty(), "render 2 had errors: {:?}", errors);
        let html = crate::dom::serialize::serialize_document(&arena);
        assert!(html.contains("<title>ok</title>"), "render 2 produced: {}", html);
    }

    #[test]
    fn test_execute_after_pool_drop_is_impossible() {
        // The pool is dropped at end of scope; we can't call execute on it
        // after drop because Rust's borrow checker prevents it. This is a
        // compile-time safety property; this test just documents it.
        let pool = JsPool::new(1).expect("pool init");
        let result = render_through_pool(&pool, "<html><body></body></html>");
        assert!(result.is_ok());
        drop(pool);
        // pool is no longer accessible past this point — verified at compile time
    }
