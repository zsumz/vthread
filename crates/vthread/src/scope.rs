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
    /// runtime.scope(|scope| {
    ///     scope.spawn("not-transferable", move || *local)?;
    ///     Ok(())
    /// }).unwrap();
    /// ```
    pub fn spawn<T, F>(&self, name: impl Into<String>, entry: F) -> Result<JoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        self.runtime.spawn(self.id, name.into(), entry)
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
    pub fn snapshot(&self) -> RuntimeSnapshot {
        self.runtime.snapshot()
    }
}

#[cfg(test)]
#[path = "scope_test.rs"]
mod scope_test;
