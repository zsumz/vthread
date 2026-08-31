//! One loss-aware failure policy shared by root and borrowed scopes.

use crate::{Error, Result, control::Shared, signal::lock};
use std::{fmt, sync::Arc};

/// Caller-owned failures returned after a structured scope reclaims its children.
/// Body, inherited policy, cleanup and one representative child remain independently
/// inspectable. Further child/cleanup failures increment counters instead of a list.
/// Original error sources remain caller-owned; runtime snapshots retain inert reports.
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
        self.retain_report(shared, false);
        Err(self.into_error())
    }

    pub(crate) fn finish_generic<R, E>(
        mut self,
        body: std::result::Result<R, E>,
        policy: Result<()>,
        shared: &Shared,
    ) -> std::result::Result<R, crate::error::ScopeRunError<E>> {
        use crate::error::ScopeRunError;
        self.policy = policy.err();
        if body.is_ok() && self.primary().is_none() {
            return body.map_err(ScopeRunError::Body);
        }
        self.retain_report(shared, body.is_err());
        let runtime = self.primary().is_some().then(|| self.into_error());
        match (body, runtime) {
            (Ok(value), None) => Ok(value),
            (Err(body), None) => Err(ScopeRunError::Body(body)),
            (Ok(_), Some(runtime)) => Err(ScopeRunError::Runtime(runtime)),
            (Err(body), Some(runtime)) => Err(ScopeRunError::BodyAndRuntime { body, runtime }),
        }
    }

    pub(crate) fn failure_count(&self) -> usize {
        [self.body(), self.policy(), self.cleanup(), self.child()]
            .iter()
            .filter(|error| error.is_some())
            .count()
            .saturating_add(usize::from(self.body_panicked))
            .saturating_add(self.additional_child_failures)
            .saturating_add(self.additional_cleanup_failures)
    }

    fn into_error(mut self) -> Error {
        if self.failure_count() == 1 {
            self.body
                .take()
                .or_else(|| self.policy.take())
                .or_else(|| self.cleanup.take())
                .or_else(|| self.child.take())
                .expect("one recorded failure")
        } else {
            Error::ScopeFailed(Arc::new(self))
        }
    }

    pub(crate) fn unwind(
        mut self,
        payload: Box<dyn std::any::Any + Send>,
        policy: Result<()>,
        shared: &Shared,
    ) -> ! {
        self.policy = policy.err();
        self.body_panicked = true;
        self.retain_report(shared, false);
        std::panic::resume_unwind(payload)
    }

    fn retain_report(&self, shared: &Shared, application_error: bool) {
        let report =
            crate::scope_failure_report::ScopeFailureReport::capture(self, application_error);
        // Both the old and new report contain only inert, bounded data.
        *lock(&shared.last_scope_failure) = Some(Arc::new(report));
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
