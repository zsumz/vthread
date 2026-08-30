//! Structured task ownership.

use std::marker::PhantomData;

use crate::{JoinHandle, Result, Runtime, RuntimeSnapshot};

/// A lexical owner for carrier-local virtual threads.
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
    /// This scope is carrier-local, so the closure and result do not require
    /// `Send`. They must be `'static`.
    pub fn spawn<'scope, T, F>(
        &'scope self,
        name: impl Into<String>,
        entry: F,
    ) -> Result<JoinHandle<'scope, T>>
    where
        F: FnOnce() -> T + 'static,
        T: 'static,
    {
        self.runtime.spawn(self.id, name.into(), entry)
    }

    /// Returns diagnostics for all tasks currently retained by the runtime.
    pub fn snapshot(&self) -> RuntimeSnapshot {
        self.runtime.snapshot()
    }
}

#[cfg(test)]
#[path = "scope_test.rs"]
mod scope_test;
