//! Runtime lifecycle and structured ownership of persistent carrier threads.

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
    workers: Mutex<Vec<thread::JoinHandle<()>>>,
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
        let runtime = Self {
            config,
            shared,
            workers: Mutex::new(Vec::new()),
        };
        for index in 0..config.carriers() {
            let shared = Arc::clone(&runtime.shared);
            let name = format!("vthread-carrier-{index}");
            let worker = thread::Builder::new()
                .name(name)
                .spawn(move || carrier::run(shared, CarrierId(index)))
                .map_err(Error::CarrierStart)?;
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
    /// Only one scope may be active per runtime. Runtime operations that wait for
    /// children are rejected inside a virtual thread.
    pub fn scope<R>(&self, body: impl FnOnce(&Scope<'_>) -> Result<R>) -> Result<R> {
        if context::current().is_some() {
            return Err(Error::InsideVThread);
        }
        let id = self.shared.begin_scope()?;
        let scope = Scope::new(self, id);
        let result = catch_unwind(AssertUnwindSafe(|| body(&scope)));
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

    /// Stops admission, reclaims tasks at their next runtime boundary, and joins carriers.
    ///
    /// This cannot preempt CPU loops, native blocking calls, or FFI. Such work may
    /// delay shutdown indefinitely. Calling shutdown inside a virtual thread is rejected.
    pub fn shutdown(&self) -> Result<()> {
        if context::current().is_some() {
            return Err(Error::InsideVThread);
        }
        self.shared.request_stop();
        self.join_workers();
        Ok(())
    }

    pub(crate) fn spawn<'scope, T: Send + 'static, F: FnOnce() -> T + Send + 'static>(
        &'scope self,
        scope: u64,
        name: String,
        entry: F,
    ) -> Result<JoinHandle<'scope, T>> {
        let spawned = self.shared.submit(scope, name, entry)?;
        Ok(JoinHandle::new(
            self,
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
        self.join_workers();
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
