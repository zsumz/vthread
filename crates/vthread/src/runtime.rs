//! Runtime lifecycle and structured ownership of persistent carrier threads.

#[path = "runtime_build.rs"]
mod runtime_build;
#[path = "runtime_lifecycle.rs"]
mod runtime_lifecycle;
#[path = "runtime_scope.rs"]
mod runtime_scope;
pub use runtime_lifecycle::ShutdownOutcome;

use crate::{Result, RuntimeBuilder, RuntimeConfig, RuntimeSnapshot, context, control::Shared};
use std::{fmt, sync::Arc};

/// An application lifecycle owner with one active root scope and persistent affine carriers.
/// Explicit supervisors may coexist with that root; independent roots use separate runtimes.
/// Task groups and supervisors share this runtime's workers, rather than creating new ones.
/// Each runtime owns `carriers + blocking_threads + 2` OS threads (five by default), plus
/// one process-shared lifecycle owner. See [`RuntimeBuilder`] for thread/stack costs and
/// [`crate::lifecycle::LIFECYCLE_CAPACITY`] for the fixed process admission bound.
///
/// Root callbacks run on their ordinary OS callers, not on runtime workers. Shutdown
/// reclaims runtime-owned children and joins workers, services and the coordinator;
/// a concurrent root callback may continue afterward. Its `run_scope` invocation
/// returns only when that callback returns. Shutdown cannot forcibly terminate it.
pub struct Runtime {
    config: RuntimeConfig,
    pub(crate) shared: Arc<Shared>,
    shutdown_driver: runtime_lifecycle::ShutdownDriver,
}

impl Runtime {
    /// Returns a builder with bounded defaults and one persistent carrier.
    pub fn builder() -> RuntimeBuilder {
        RuntimeBuilder::default()
    }

    /// Creates a runtime with default configuration.
    /// Native/OS initialization may block indefinitely; bounded service startup needs
    /// a process-level watchdog. The readiness handshake has no wall-clock timeout.
    /// Partial initialization is explicitly shut down. If both construction and cleanup
    /// fail, [`crate::Error::ConstructionFailed`] retains both causes.
    pub fn new() -> Result<Self> {
        Self::builder().build()
    }

    /// Returns the process-unique runtime identity used by diagnostics.
    pub fn id(&self) -> crate::diagnostics::RuntimeId {
        self.shared.id
    }

    /// Returns the immutable runtime configuration.
    pub fn config(&self) -> RuntimeConfig {
        self.config
    }

    /// Returns a weakly consistent view of carrier state and retained task diagnostics.
    /// Components can advance independently while this view is assembled. Task and
    /// service observation does not hold the global admission/completion lock.
    pub fn snapshot(&self) -> RuntimeSnapshot {
        self.shared.snapshot()
    }

    #[cfg(test)]
    pub(crate) fn spawn<T: Send + 'static>(
        &self,
        scope: u64,
        name: String,
        entry: impl FnOnce() -> T + Send + 'static,
    ) -> Result<crate::JoinHandle<T>> {
        let spawned = self.shared.submit(scope, name, entry)?;
        Ok(crate::JoinHandle::new(
            Arc::clone(&self.shared),
            spawned.id,
            spawned.name,
            spawned.cell,
            spawned.record,
        ))
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        // Also release a coordinator created before a partial construction failure.
        self.shutdown_driver.ready(&self.shared);
        self.shared.request_stop();
        // Any managed worker may participate in a cross-runtime dependency cycle.
        // The process lifecycle owner already retains this operation and all handles.
        if context::current().is_none() && !crate::worker_context::is_managed() {
            let _ = self.wait_shutdown(None);
        }
    }
}

impl fmt::Debug for Runtime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Runtime")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "runtime_test.rs"]
mod runtime_test;
