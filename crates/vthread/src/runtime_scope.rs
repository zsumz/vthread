//! Root scope admission, callback ownership, and structured reclamation.

use crate::{Error, Result, Runtime, Scope, context, control::Shared, error::ScopeRunError};
use std::panic::{AssertUnwindSafe, catch_unwind};

impl Runtime {
    /// Runs a scope body on an ordinary OS caller and drains all admitted children.
    ///
    /// One lexical root scope may be active per runtime; supervisors may coexist.
    /// Virtual callers use local_scope for borrowed, nested ownership.
    pub fn run_scope<R>(&self, body: impl FnOnce(&Scope<'_>) -> Result<R>) -> Result<R> {
        self.run_scope_with(crate::ScopeOptions::default(), body)
    }

    /// Runs a root with an inherited task deadline, observed before and after the callback.
    /// The OS callback cannot be preempted. Child reclamation may exceed the deadline;
    /// full failures are returned to the caller; the latest snapshot retains an inert report.
    pub fn run_scope_with<R>(
        &self,
        options: crate::ScopeOptions,
        body: impl FnOnce(&Scope<'_>) -> Result<R>,
    ) -> Result<R> {
        self.with_scope(options, body, crate::ScopeFailure::finish)?
    }

    /// Runs a root callback with a caller-owned application error.
    /// Body failure cancels children before reclamation. Application errors need not
    /// implement Send, Display, Error, or static lifetimes and are never retained.
    ///
    /// ```
    /// use vthread::{Runtime, error::ScopeRunError};
    /// let runtime = Runtime::new()?;
    /// let domain_error = String::from("invalid application input");
    /// let result = runtime.try_run_scope(|_| Err::<(), _>(&domain_error));
    /// assert!(matches!(result, Err(ScopeRunError::Body(error)) if error == &domain_error));
    /// runtime.shutdown()?;
    /// # Ok::<(), vthread::Error>(())
    /// ```
    pub fn try_run_scope<R, E>(
        &self,
        body: impl FnOnce(&Scope<'_>) -> std::result::Result<R, E>,
    ) -> std::result::Result<R, ScopeRunError<E>> {
        self.try_run_scope_with(crate::ScopeOptions::default(), body)
    }

    /// Runs a generic-error root with the same inherited deadline as run_scope_with.
    pub fn try_run_scope_with<R, E>(
        &self,
        options: crate::ScopeOptions,
        body: impl FnOnce(&Scope<'_>) -> std::result::Result<R, E>,
    ) -> std::result::Result<R, ScopeRunError<E>> {
        self.with_scope(options, body, crate::ScopeFailure::finish_generic)
            .map_err(ScopeRunError::Runtime)?
    }

    fn with_scope<R, E, O>(
        &self,
        options: crate::ScopeOptions,
        body: impl FnOnce(&Scope<'_>) -> std::result::Result<R, E>,
        finish: impl FnOnce(crate::ScopeFailure, std::result::Result<R, E>, Result<()>, &Shared) -> O,
    ) -> Result<O> {
        if context::current().is_some() {
            return Err(Error::InsideVThread);
        }
        if crate::worker_context::is_managed() {
            return Err(Error::InsideManagedWorker);
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
            Ok(result) => Ok(finish(failures, result, policy, &self.shared)),
        }
    }
}

fn root_deadline(deadline: Option<std::time::Instant>) -> Result<()> {
    if deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
        Err(Error::DeadlineExceeded)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "runtime_scope_test.rs"]
mod runtime_scope_test;
