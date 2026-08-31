//! Runtime lifecycle and structured ownership of persistent carrier threads.

#[path = "runtime_lifecycle.rs"]
mod runtime_lifecycle;
#[path = "runtime_scope.rs"]
mod runtime_scope;
pub use runtime_lifecycle::ShutdownOutcome;

use crate::{
    CarrierId, Error, JoinHandle, Result, RuntimeBuilder, RuntimeConfig, RuntimeSnapshot, carrier,
    context, control::Shared, signal::lock,
};
use std::{
    fmt,
    sync::{Arc, Mutex},
    thread,
};

/// An application lifecycle owner with one active root scope and persistent affine carriers.
/// Explicit supervisors may coexist with that root; independent roots use separate runtimes.
/// Task groups and supervisors share this runtime's workers, rather than creating new ones.
/// Each runtime owns `carriers + blocking_threads + 2` OS threads (five by default), plus
/// one process-shared lifecycle owner. See [`RuntimeBuilder`] for thread/stack costs and
/// [`crate::lifecycle::LIFECYCLE_CAPACITY`] for the fixed process admission bound.
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
    pub fn new() -> Result<Self> {
        Self::builder().build()
    }

    pub(crate) fn from_config(config: RuntimeConfig) -> Result<Self> {
        if context::current().is_some() {
            return Err(Error::InsideVThread);
        }
        if crate::worker_context::is_managed() {
            return Err(Error::InsideManagedWorker);
        }
        let shared = Arc::new(Shared::new(config));
        let workers = Arc::new(Mutex::new(Vec::new()));
        let shutdown_driver = runtime_lifecycle::ShutdownDriver::new(&shared, &workers)?;
        let runtime = Self {
            config,
            shared,
            shutdown_driver,
        };
        runtime
            .shared
            .services
            .set(crate::services::Services::new(
                config,
                Arc::downgrade(&runtime.shared),
            )?)
            .map_err(|_| {
                Error::fault(
                    crate::error::FaultComponent::Lifecycle,
                    "runtime services initialized twice",
                )
            })?;
        for index in 0..config.carriers() {
            let shared = Arc::clone(&runtime.shared);
            let name = format!("vthread-carrier-{index}");
            let worker = thread::Builder::new()
                .name(name)
                .spawn(move || {
                    crate::worker_context::attach(
                        Arc::downgrade(&shared),
                        crate::ThreadComponent::Carrier,
                    );
                    carrier::run(Arc::clone(&shared), CarrierId(index));
                    #[cfg(test)]
                    if let Some(hook) = lock(&shared.carrier_exit_hook).take() {
                        hook();
                    }
                })
                .map_err(|error| Error::thread_start(crate::ThreadComponent::Carrier, error))?;
            lock(&workers).push(worker);
        }
        runtime.shutdown_driver.ready(&runtime.shared);
        crate::lifecycle_owner::check_health()?;
        Ok(runtime)
    }

    /// Returns the process-unique runtime identity used by diagnostics.
    pub fn id(&self) -> crate::diagnostics::RuntimeId {
        self.shared.id
    }

    /// Returns the immutable runtime configuration.
    pub fn config(&self) -> RuntimeConfig {
        self.config
    }

    /// Returns the published carrier state and retained task diagnostics.
    pub fn snapshot(&self) -> RuntimeSnapshot {
        self.shared.snapshot()
    }

    pub(crate) fn spawn<T: Send + 'static, F: FnOnce() -> T + Send + 'static>(
        &self,
        scope: u64,
        name: String,
        entry: F,
    ) -> Result<JoinHandle<T>> {
        let spawned = self.shared.submit(scope, name, entry)?;
        Ok(JoinHandle::new(
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
