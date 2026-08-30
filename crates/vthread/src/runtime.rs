//! Runtime lifecycle and carrier driving.

use std::{
    cell::{Cell, RefCell},
    fmt,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
};

use crate::{
    Error, JoinHandle, Result, RuntimeBuilder, RuntimeConfig, RuntimeSnapshot, Scope, TaskId,
    kernel::Kernel,
};

/// A single-carrier virtual-thread runtime.
pub struct Runtime {
    config: RuntimeConfig,
    kernel: RefCell<Kernel>,
    active_scope: Cell<bool>,
    next_scope: Cell<u64>,
}

impl Runtime {
    /// Returns a builder with bounded defaults.
    pub fn builder() -> RuntimeBuilder {
        RuntimeBuilder::default()
    }

    /// Creates a runtime with default configuration.
    pub fn new() -> Result<Self> {
        Self::builder().build()
    }

    pub(crate) fn from_config(config: RuntimeConfig) -> Self {
        Self {
            config,
            kernel: RefCell::new(Kernel::new(config)),
            active_scope: Cell::new(false),
            next_scope: Cell::new(1),
        }
    }

    /// Returns the immutable runtime configuration.
    pub fn config(&self) -> RuntimeConfig {
        self.config
    }

    /// Runs one structured scope on the caller thread.
    pub fn scope<R>(&self, body: impl FnOnce(&Scope<'_>) -> Result<R>) -> Result<R> {
        if self.active_scope.replace(true) {
            return Err(Error::NestedScope);
        }
        let _active = ActiveScopeGuard(&self.active_scope);
        let scope_id = self.next_scope.get();
        let next_scope = scope_id
            .checked_add(1)
            .ok_or(Error::Invariant("scope id space exhausted"))?;
        self.next_scope.set(next_scope);
        let scope = Scope::new(self, scope_id);
        let body_result = catch_unwind(AssertUnwindSafe(|| body(&scope)));
        let drain_result = self.drain_scope(scope_id);
        let unobserved = if drain_result.is_ok() {
            self.kernel.borrow().unobserved_panic(scope_id)
        } else {
            None
        };
        let abort_result = if drain_result.is_err() {
            self.kernel.borrow_mut().abort_scope(scope_id)
        } else {
            Ok(())
        };
        self.kernel.borrow_mut().purge_scope(scope_id);

        match body_result {
            Err(payload) => {
                let _ = drain_result;
                let _ = abort_result;
                resume_unwind(payload)
            }
            Ok(Err(error)) => {
                abort_result?;
                drain_result?;
                Err(error)
            }
            Ok(Ok(value)) => {
                abort_result?;
                drain_result?;
                if let Some((task, name, panic)) = unobserved {
                    return Err(Error::task_panicked(task, name, panic));
                }
                Ok(value)
            }
        }
    }

    /// Returns a point-in-time scheduler and task snapshot.
    pub fn snapshot(&self) -> RuntimeSnapshot {
        self.kernel.borrow().snapshot()
    }

    pub(crate) fn spawn<'scope, T, F>(
        &'scope self,
        scope: u64,
        name: String,
        entry: F,
    ) -> Result<JoinHandle<'scope, T>>
    where
        F: FnOnce() -> T + 'static,
        T: 'static,
    {
        let spawned = self.kernel.borrow_mut().spawn(scope, name, entry)?;
        Ok(JoinHandle::new(
            self,
            spawned.id,
            spawned.name,
            spawned.cell,
            spawned.record,
        ))
    }

    pub(crate) fn run_until(&self, task: TaskId) -> Result<()> {
        loop {
            if self.kernel.borrow().is_terminal(task)? {
                return Ok(());
            }
            let progressed = self.kernel.borrow_mut().tick()?;
            if !progressed {
                let active = self.kernel.borrow().snapshot().active;
                return Err(Error::RuntimeStalled { active });
            }
        }
    }

    fn drain_scope(&self, scope: u64) -> Result<()> {
        loop {
            let active = self.kernel.borrow().active_in_scope(scope);
            if active == 0 {
                return Ok(());
            }
            let progressed = self.kernel.borrow_mut().tick()?;
            if !progressed {
                return Err(Error::RuntimeStalled { active });
            }
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

struct ActiveScopeGuard<'runtime>(&'runtime Cell<bool>);

impl Drop for ActiveScopeGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

#[cfg(test)]
#[path = "runtime_test.rs"]
mod runtime_test;
