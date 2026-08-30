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
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::{Arc, Mutex},
    thread,
};

/// A bounded runtime with persistent, permanently affine carrier threads.
pub struct Runtime {
    config: RuntimeConfig,
    pub(crate) shared: Arc<Shared>,
    workers: Arc<Mutex<Vec<thread::JoinHandle<()>>>>,
    worker_ids: Vec<thread::ThreadId>,
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
        let shared = Arc::new(Shared::new(config));
        shared
            .services
            .set(crate::services::Services::new(config)?)
            .map_err(|_| Error::Invariant("runtime services initialized twice"))?;
        let mut runtime = Self {
            config,
            shared,
            workers: Arc::new(Mutex::new(Vec::new())),
            worker_ids: Vec::new(),
            shutdown_driver: runtime_lifecycle::ShutdownDriver::default(),
        };
        for index in 0..config.carriers() {
            let shared = Arc::clone(&runtime.shared);
            let name = format!("vthread-carrier-{index}");
            let worker = thread::Builder::new()
                .name(name)
                .spawn(move || carrier::run(shared, CarrierId(index)))
                .map_err(Error::CarrierStart)?;
            runtime.worker_ids.push(worker.thread().id());
            lock(&runtime.workers).push(worker);
        }
        Ok(runtime)
    }

    /// Returns the immutable runtime configuration.
    pub fn config(&self) -> RuntimeConfig {
        self.config
    }

    /// Runs a scope body on an ordinary OS caller and drains all admitted children.
    ///
    /// One lexical root scope may be active per runtime; supervisors may coexist.
    /// Virtual callers use local_scope for borrowed, nested ownership.
    pub fn scope<R>(&self, body: impl FnOnce(&Scope<'_>) -> Result<R>) -> Result<R> {
        self.scope_with(crate::ScopeOptions::default(), body)
    }

    /// Runs a structured scope with an optional inherited monotonic deadline.
    pub fn scope_with<R>(
        &self,
        options: crate::ScopeOptions,
        body: impl FnOnce(&Scope<'_>) -> Result<R>,
    ) -> Result<R> {
        if context::current().is_some() {
            return Err(Error::InsideVThread);
        }
        let id = self.shared.begin_owned(options, false)?;
        let scope = Scope::new(self, id);
        let result = catch_unwind(AssertUnwindSafe(|| body(&scope)));
        if !matches!(&result, Ok(Ok(_))) {
            scope.cancel();
        }
        let drained = self.shared.wait(id, None);
        let unobserved = self.shared.unobserved(id);
        self.shared.finish_scope(id);
        match result {
            Err(payload) => resume_unwind(payload),
            Ok(result) => {
                drained?;
                let value = result?;
                unobserved?;
                Ok(value)
            }
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

    fn join_workers(&self) {
        let mut workers = lock(&self.workers);
        for worker in workers.drain(..) {
            // A task may drop the last Arc<Runtime>. Its own carrier exits after that
            // task returns; joining itself here would deadlock.
            if worker.thread().id() != thread::current().id() {
                let _ = worker.join();
            }
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.shared.request_stop();
        // The final Arc can be owned by one of our workers. A coordinator retains
        // and drains that work after this destructor returns to the worker.
        if self.owns_current_worker() && self.start_shutdown_driver().is_ok() {
            return;
        }
        self.join_workers();
        if let Some(services) = self.shared.services.get() {
            services.join();
        }
        self.join_shutdown_driver();
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
