//! Dedicated V8 executor pool.
//!
//! Each `JsPool` owns N OS threads. Each thread owns one long-lived
//! `v8::OwnedIsolate`. Render jobs are dispatched round-robin via mpsc;
//! callers block on a per-job reply channel until the executor returns.
//!
//! ## Why this exists
//!
//! V8 isolates are `Send` but `!Sync` — only one thread can use an isolate
//! at a time. Pinning each isolate to a dedicated thread avoids `v8::Locker`
//! overhead and gives clean ownership semantics.
//!
//! HTTP fetch and HTML parse stay on the caller thread (parallel via tokio
//! and Rust threading); only V8 execution dispatches through the pool.
//!
//! ## Threading safety
//!
//! `Arena` is `Send` (SlotMap + SharedRwLock + plain types, no Rc/RefCell).
//! We pass it to the executor by raw pointer wrapped in `SendPtr<Arena>`.
//! Safety relies on the caller thread being blocked on the reply channel
//! for the entire job lifetime — the arena cannot be dropped while the
//! executor holds the pointer because the synchronous wait ensures the
//! caller's stack frame is still live.

use std::sync::{Arc, LazyLock, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::dom::arena::Arena;
use crate::error::EngineError;
use crate::net::fetch::FetchContext;

/// Default per-render JS execution timeout (configurable via `with_timeout`).
pub const DEFAULT_JS_TIMEOUT: Duration = Duration::from_secs(10);

/// `*mut Arena` wrapped to satisfy the `Send` requirement for channel transfer.
///
/// SAFETY: the caller thread blocks on the reply channel until the executor
/// signals completion. While the executor holds this pointer, the caller's
/// stack frame containing the `Arena` is still live and the pointer remains
/// valid. Only one thread accesses the arena at a time (caller before send,
/// executor after receive, caller after recv reply).
pub struct SendPtr<T>(pub *mut T);

unsafe impl<T> Send for SendPtr<T> {}

/// Job dispatched to an executor thread.
enum Job {
    Execute {
        arena: SendPtr<Arena>,
        base_url: Option<String>,
        fetch_context: FetchContext,
        reply: SyncSender<Result<Vec<String>, EngineError>>,
    },
    Shutdown,
}

/// Pool of V8 executor threads.
pub struct JsPool {
    senders: Vec<SyncSender<Job>>,
    /// Thread-safe handles for each worker's isolate, used to fire
    /// `terminate_execution` from the watchdog when JS exceeds the timeout.
    /// Index parallels `senders`.
    isolate_handles: Vec<v8::IsolateHandle>,
    next: AtomicUsize,
    js_timeout: Duration,
    /// Held until Drop sends Shutdown to each worker, then joins.
    threads: Mutex<Vec<JoinHandle<()>>>,
}

impl JsPool {
    /// Spawn `workers` executor threads with the default JS timeout (10 s).
    /// Blocks until each thread has initialized its V8 isolate.
    pub fn new(workers: usize) -> Result<Self, EngineError> {
        Self::with_timeout(workers, DEFAULT_JS_TIMEOUT)
    }

    /// Spawn `workers` executor threads with a custom per-render JS timeout.
    /// A render whose JS phase exceeds `js_timeout` is killed via
    /// `IsolateHandle::terminate_execution`; the executor recovers and the
    /// next render proceeds normally.
    ///
    /// Each worker owns one long-lived isolate pinned to its OS thread.
    /// v8 14.7 fixed the `JSDispatchTable` freelist race that plagued v8 135,
    /// so multiple isolates run concurrently with no global serialization.
    pub fn with_timeout(workers: usize, js_timeout: Duration) -> Result<Self, EngineError> {
        let workers = workers.max(1);
        log::info!(
            "starting JsPool with {} worker(s), {:?} JS timeout...",
            workers, js_timeout,
        );
        let t0 = std::time::Instant::now();

        let mut senders = Vec::with_capacity(workers);
        let mut isolate_handles = Vec::with_capacity(workers);
        let mut threads = Vec::with_capacity(workers);

        for worker_id in 0..workers {
            // Bounded channel: cap = 2 prevents callers from queuing
            // unbounded backlogs into a stuck worker.
            let (tx, rx) = mpsc::sync_channel::<Job>(2);
            let (ready_tx, ready_rx) = mpsc::sync_channel::<v8::IsolateHandle>(0);
            let handle = std::thread::Builder::new()
                .name(format!("blazeweb-js-{}", worker_id))
                .spawn(move || executor_main(worker_id, rx, ready_tx))
                .map_err(|e| EngineError::JsExecution {
                    message: format!("failed to spawn executor thread: {e}"),
                    stack: None,
                })?;
            // Wait for worker to confirm isolate is ready, retrieve its
            // thread-safe handle for the watchdog.
            let isolate_handle = ready_rx.recv().map_err(|_| EngineError::JsExecution {
                message: "executor thread died during startup".into(),
                stack: None,
            })?;
            senders.push(tx);
            isolate_handles.push(isolate_handle);
            threads.push(handle);
        }

        log::info!("JsPool ready ({} workers, {:?})", workers, t0.elapsed());
        Ok(Self {
            senders,
            isolate_handles,
            next: AtomicUsize::new(0),
            js_timeout,
            threads: Mutex::new(threads),
        })
    }

    /// Submit a render job and block until the executor returns.
    ///
    /// Round-robin dispatches across worker threads. The caller's thread is
    /// parked on the reply channel — other Python threads may run via the
    /// released GIL.
    ///
    /// If JS execution exceeds the pool's `js_timeout`, we fire
    /// `IsolateHandle::terminate_execution` on the worker's isolate. V8 throws
    /// an uncatchable exception inside the running script, the executor
    /// returns errors, and the isolate self-recovers via
    /// `cancel_terminate_execution` at the end of the executor loop.
    pub fn execute(
        &self,
        arena: *mut Arena,
        base_url: Option<&str>,
        fetch_context: FetchContext,
    ) -> Result<Vec<String>, EngineError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        let idx = self.next.fetch_add(1, Ordering::Relaxed) % self.senders.len();
        let job = Job::Execute {
            arena: SendPtr(arena),
            base_url: base_url.map(String::from),
            fetch_context,
            reply: reply_tx,
        };
        self.senders[idx]
            .send(job)
            .map_err(|_| EngineError::JsExecution {
                message: "JS executor pool shut down".into(),
                stack: None,
            })?;

        match reply_rx.recv_timeout(self.js_timeout) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => {
                log::warn!(
                    "[js-{}] JS exceeded {:?} timeout, terminating execution",
                    idx, self.js_timeout,
                );
                self.isolate_handles[idx].terminate_execution();
                // Executor will return shortly with an error reply.
                reply_rx.recv().map_err(|_| EngineError::JsExecution {
                    message: "JS executor dropped reply after termination".into(),
                    stack: None,
                })?
            }
            Err(RecvTimeoutError::Disconnected) => Err(EngineError::JsExecution {
                message: "JS executor dropped reply (panic or shutdown)".into(),
                stack: None,
            }),
        }
    }
}

impl Drop for JsPool {
    fn drop(&mut self) {
        log::debug!("JsPool shutting down ({} workers)...", self.senders.len());
        for tx in &self.senders {
            let _ = tx.send(Job::Shutdown);
        }
        if let Ok(mut threads) = self.threads.lock() {
            for handle in threads.drain(..) {
                let _ = handle.join();
            }
        }
        log::debug!("JsPool shutdown complete");
    }
}

/// Executor thread main loop. Creates its own isolate (must be created on
/// the same thread that owns it — V8 invariant: `Isolate::Drop` asserts
/// `GetCurrent()` matches the dropping thread).
fn executor_main(
    worker_id: usize,
    rx: Receiver<Job>,
    ready_tx: SyncSender<v8::IsolateHandle>,
) {
    log::debug!("[js-{}] starting...", worker_id);
    let mut isolate = super::runtime::create_isolate_for_worker();
    let handle = isolate.thread_safe_handle();
    log::debug!("[js-{}] isolate ready, signaling startup", worker_id);
    let _ = ready_tx.send(handle);

    while let Ok(job) = rx.recv() {
        match job {
            Job::Shutdown => {
                log::debug!("[js-{}] shutdown received, exiting", worker_id);
                break;
            }
            Job::Execute { arena, base_url, fetch_context, reply } => {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    // SAFETY: caller thread is blocked on `reply` channel
                    // recv, so the arena cannot be dropped while we hold this
                    // pointer.
                    let arena_ref = unsafe { &mut *arena.0 };
                    super::runtime::run_one_render(
                        &mut isolate,
                        arena_ref,
                        base_url.as_deref(),
                        &fetch_context,
                    )
                }));
                let result = result.unwrap_or_else(|_| {
                    log::error!("[js-{}] executor panic during render", worker_id);
                    Err(EngineError::JsExecution {
                        message: "executor panic during render".into(),
                        stack: None,
                    })
                });

                // If the watchdog fired terminate_execution, V8 leaves the
                // isolate in a "terminating" state. Clear it so the next job
                // can run normally.
                if isolate.is_execution_terminating() {
                    log::debug!("[js-{}] clearing termination state", worker_id);
                    isolate.cancel_terminate_execution();
                }

                if reply.send(result).is_err() {
                    log::warn!("[js-{}] reply channel dropped (caller gone)", worker_id);
                }
            }
        }
    }
    log::debug!("[js-{}] exited; isolate dropping", worker_id);
}

/// Process-global single-worker default pool, eager-initialized on first call.
///
/// Used by module-level `blazeweb.render()` / `blazeweb.fetch()` and by the
/// current internal `js::runtime::execute_scripts` until per-Client pools are
/// wired in step 7.
static DEFAULT_POOL: LazyLock<Arc<JsPool>> = LazyLock::new(|| {
    Arc::new(JsPool::new(1).expect("default JsPool init failed"))
});

/// Get a reference to the process-global default pool.
pub fn default_pool() -> Arc<JsPool> {
    Arc::clone(&DEFAULT_POOL)
}

#[cfg(test)]
#[path = "executor_tests.rs"]
mod tests;
