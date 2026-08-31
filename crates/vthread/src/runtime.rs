//! Runtime lifecycle and structured ownership of persistent carrier threads.

#[path = "runtime_lifecycle.rs"]
mod runtime_lifecycle;
pub use runtime_lifecycle::ShutdownOutcome;

use crate::{
    CarrierId, Error, JoinHandle, Result, RuntimeBuilder, RuntimeConfig, RuntimeSnapshot, Scope,
    carrier, context, control::Shared, signal::lock,
};
use std::{
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex},
    thread,
};

/// An application lifecycle owner with one active root scope and persistent affine carriers.
/// Explicit supervisors may coexist with that root; independent roots use separate runtimes.
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
            return Err(Error::InsideBlockingWorker);
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

    /// Runs a scope body on an ordinary OS caller and drains all admitted children.
    ///
    /// One lexical root scope may be active per runtime; supervisors may coexist.
    /// Virtual callers use local_scope for borrowed, nested ownership.
    pub fn run_scope<R>(&self, body: impl FnOnce(&Scope<'_>) -> Result<R>) -> Result<R> {
        self.run_scope_with(crate::ScopeOptions::default(), body)
    }

    /// Runs a root with an inherited task deadline, observed before and after the callback.
    /// The OS callback cannot be preempted. Child reclamation may exceed the deadline;
    /// failures are retained in ScopeFailure and the latest runtime snapshot.
    pub fn run_scope_with<R>(
        &self,
        options: crate::ScopeOptions,
        body: impl FnOnce(&Scope<'_>) -> Result<R>,
    ) -> Result<R> {
        if context::current().is_some() {
            return Err(Error::InsideVThread);
        }
        if crate::worker_context::is_managed() {
            return Err(Error::InsideBlockingWorker);
        }
        root_deadline(options.deadline)?;
        let id = self.shared.begin_owned(options, false)?;
        let scope = Scope::new(self, id);
        let result = catch_unwind(AssertUnwindSafe(|| body(&scope)));
        let policy = root_deadline(options.deadline);
        if !matches!(&result, Ok(Ok(_))) || policy.is_err() {
            scope.cancel();
        }
        let drained = self.shared.wait(id, None);
        let mut failures = self.shared.unobserved(id);
        if let Err(error) = drained {
            failures.cleanup_failed(error);
        }
        let policy = policy.and_then(|()| root_deadline(options.deadline));
        self.shared.finish_scope(id);
        match result {
            Err(payload) => failures.unwind(payload, policy, &self.shared),
            Ok(result) => failures.finish(result, policy, &self.shared),
        }
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

fn root_deadline(deadline: Option<std::time::Instant>) -> Result<()> {
    if deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
        Err(Error::DeadlineExceeded)
    } else {
        Ok(())
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
