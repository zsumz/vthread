//! One loss-aware failure policy shared by root and borrowed scopes.

use crate::{Error, Result, control::Shared, signal::lock};
use std::{fmt, sync::Arc};

/// Bounded failures retained after a structured scope reclaims its children.
/// Body, inherited policy, cleanup and one representative child remain independently
/// inspectable. Further child/cleanup failures increment counters instead of a list.
#[derive(Debug, Default)]
pub struct ScopeFailure {
    body: Option<Error>,
    policy: Option<Error>,
    cleanup: Option<Error>,
    child: Option<Error>,
    additional_child_failures: usize,
    additional_cleanup_failures: usize,
    body_panicked: bool,
}

impl ScopeFailure {
    /// Error returned by the scope callback, if any.
    pub fn body(&self) -> Option<&Error> {
        self.body.as_ref()
    }
    /// Deadline/cancellation observed before cleanup changed cancellation state.
    pub fn policy(&self) -> Option<&Error> {
        self.policy.as_ref()
    }
    /// First failure while waiting for reclamation.
    pub fn cleanup(&self) -> Option<&Error> {
        self.cleanup.as_ref()
    }
    /// First unobserved child failure in task identity order.
    pub fn child(&self) -> Option<&Error> {
        self.child.as_ref()
    }
    /// Number of additional unobserved failed children.
    pub fn additional_child_failures(&self) -> usize {
        self.additional_child_failures
    }
    /// Number of additional cleanup failures.
    pub fn additional_cleanup_failures(&self) -> usize {
        self.additional_cleanup_failures
    }
    /// Whether the body unwound. Its original payload is rethrown after reclamation.
    pub fn body_panicked(&self) -> bool {
        self.body_panicked
    }
    /// Representative error order: body, inherited policy, cleanup, then child.
    /// This selection never removes the other recorded failures.
    pub fn primary(&self) -> Option<&Error> {
        self.body()
            .or(self.policy())
            .or(self.cleanup())
            .or(self.child())
    }

    pub(crate) fn child_failed(&mut self, error: Error) {
        if self.child.is_none() {
            self.child = Some(error);
        } else {
            self.additional_child_failures = self.additional_child_failures.saturating_add(1);
        }
    }

    pub(crate) fn cleanup_failed(&mut self, error: Error) {
        if self.cleanup.is_none() {
            self.cleanup = Some(error);
        } else {
            self.additional_cleanup_failures = self.additional_cleanup_failures.saturating_add(1);
        }
    }

    pub(crate) fn finish<R>(
        mut self,
        body: Result<R>,
        policy: Result<()>,
        shared: &Shared,
    ) -> Result<R> {
        self.policy = policy.err();
        let value = match body {
            Ok(value) => Some(value),
            Err(error) => {
                self.body = Some(error);
                None
            }
        };
        if self.primary().is_none() {
            return Ok(value.expect("successful body"));
        }
        let failure = self.retain(shared);
        Err(Error::ScopeFailed(failure))
    }

    pub(crate) fn unwind(
        mut self,
        payload: Box<dyn std::any::Any + Send>,
        policy: Result<()>,
        shared: &Shared,
    ) -> ! {
        self.policy = policy.err();
        self.body_panicked = true;
        self.retain(shared);
        std::panic::resume_unwind(payload)
    }

    fn retain(self, shared: &Shared) -> Arc<Self> {
        let failure = Arc::new(self);
        let previous = lock(&shared.last_scope_failure).replace(Arc::clone(&failure));
        // An owned I/O cause may run arbitrary user Drop code. Never do so under metadata locks.
        drop(previous);
        failure
    }
}

impl fmt::Display for ScopeFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(primary) = self.primary() {
            write!(f, "scope failed: {primary}")
        } else {
            f.write_str("scope body panicked; failures retained in runtime diagnostics")
        }
    }
}

impl std::error::Error for ScopeFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.primary().map(|error| error as &dyn std::error::Error)
    }
}

#[cfg(test)]
#[path = "scope_failure_test.rs"]
mod scope_failure_test;
