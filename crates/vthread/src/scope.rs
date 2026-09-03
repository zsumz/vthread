//! Structured task ownership.

use std::marker::PhantomData;

use crate::{JoinHandle, Result, Runtime, RuntimeSnapshot};

/// A lexical owner for transferable tasks dispatched to persistent carriers.
pub struct Scope<'runtime> {
    runtime: &'runtime Runtime,
    id: u64,
    _invariant: PhantomData<&'runtime mut &'runtime ()>,
}

impl<'runtime> Scope<'runtime> {
    pub(crate) fn new(runtime: &'runtime Runtime, id: u64) -> Self {
        Self {
            runtime,
            id,
            _invariant: PhantomData,
        }
    }

    /// Spawns a named virtual thread owned by this scope.
    ///
    /// The closure and result must be `Send + 'static`. Non-Send values may be
    /// created inside the task and survive suspension because its stack never migrates.
    /// Use local_scope inside a virtual thread for borrowed, non-Send children.
    ///
    /// ```compile_fail
    /// let runtime = vthread::Runtime::new().unwrap();
    /// let local = std::rc::Rc::new(42);
    /// runtime.run_scope(|scope| {
    ///     scope.spawn("not-transferable", move || *local)?;
    ///     Ok(())
    /// }).unwrap();
    /// ```
    pub fn spawn<T, F>(&self, name: impl Into<String>, entry: F) -> Result<JoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        crate::Spawner::spawn_on(
            std::sync::Arc::clone(&self.runtime.shared),
            self.id,
            crate::SpawnOptions::default(),
            name,
            entry,
        )
    }

    /// Spawns a child with a deadline bounded by its owner and same-runtime caller.
    pub fn spawn_with<T: Send + 'static>(
        &self,
        options: crate::SpawnOptions,
        name: impl Into<String>,
        entry: impl FnOnce() -> T + Send + 'static,
    ) -> Result<JoinHandle<T>> {
        crate::Spawner::spawn_on(
            std::sync::Arc::clone(&self.runtime.shared),
            self.id,
            options,
            name,
            entry,
        )
    }

    /// Returns a transferable admission capability; it cannot extend this scope's lifetime.
    pub fn spawner(&self) -> crate::Spawner {
        crate::Spawner::new(&self.runtime.shared, self.id)
    }

    /// Returns the cooperative cancellation token inherited by this scope's children.
    pub fn cancellation_token(&self) -> crate::CancellationToken {
        self.runtime
            .shared
            .scope_options(self.id)
            .expect("live scope")
            .cancellation
    }

    /// Requests cooperative cancellation of every child in this scope.
    pub fn cancel(&self) {
        self.cancellation_token().cancel();
    }

    /// Returns the inherited monotonic deadline.
    pub fn deadline(&self) -> Option<std::time::Instant> {
        self.runtime
            .shared
            .scope_options(self.id)
            .expect("live scope")
            .deadline
    }

    /// Returns diagnostics for all tasks currently retained by the runtime.
    pub fn runtime_snapshot(&self) -> RuntimeSnapshot {
        self.runtime.snapshot()
    }
}

#[cfg(test)]
#[path = "scope_test.rs"]
mod scope_test;
