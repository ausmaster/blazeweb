//! Shared tokio runtime, one per process. Eager-init at module import.
//!
//! Why shared: every Client uses the same runtime. Every Python thread calling
//! Client methods calls `runtime.block_on(...)` from a non-runtime thread —
//! safe, and the runtime's worker pool naturally serves N callers concurrently.

use std::sync::Arc;

use once_cell::sync::OnceCell;
use tokio::runtime::{Builder, Runtime};

static RUNTIME: OnceCell<Arc<Runtime>> = OnceCell::new();

/// Get (or lazily init) the process-wide tokio multi-threaded runtime.
pub fn shared() -> Arc<Runtime> {
    RUNTIME
        .get_or_init(|| {
            let rt = Builder::new_multi_thread()
                .enable_all()
                // Use (CPU count) worker threads by default. tokio picks this
                // reasonably; explicitly override via TOKIO_WORKER_THREADS env.
                .thread_name("blazeweb-tokio")
                .build()
                .expect("failed to build blazeweb tokio runtime");
            Arc::new(rt)
        })
        .clone()
}
