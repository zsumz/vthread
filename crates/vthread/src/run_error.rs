//! Caller-owned body errors and lossless application-runner failures.

use crate::Error;
use std::fmt;

/// Failure of a scope whose callback uses an application-specific error type.
/// No `Send`, `Display`, or `'static` bound is required for the body error.
/// The runtime never retains or formats that value in diagnostics.
#[derive(Debug)]
#[non_exhaustive]
pub enum ScopeRunError<E> {
    /// Only the callback failed.
    Body(E),
    /// Admission, inherited policy, child reclamation, or unobserved work failed.
    Runtime(Error),
    /// Both the callback and structured runtime work failed.
    BodyAndRuntime {
        /// Original application error, owned by the caller.
        body: E,
        /// Runtime error, including any aggregated secondary failures.
        runtime: Error,
    },
}

impl<E: fmt::Display> fmt::Display for ScopeRunError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Body(body) => write!(f, "scope body failed: {body}"),
            Self::Runtime(runtime) => runtime.fmt(f),
            Self::BodyAndRuntime { body, runtime } => {
                write!(
                    f,
                    "scope body failed: {body}; runtime also failed: {runtime}"
                )
            }
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for ScopeRunError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Body(body) | Self::BodyAndRuntime { body, .. } => Some(body),
            Self::Runtime(runtime) => Some(runtime),
        }
    }
}

/// Both the application scope and explicit runtime shutdown failed.
/// Single failures are returned directly from [`crate::run`].
#[derive(Debug)]
pub struct RunFailure {
    scope: Error,
    shutdown: Error,
}

impl RunFailure {
    pub(crate) fn new(scope: Error, shutdown: Error) -> Self {
        Self { scope, shutdown }
    }
    /// Scope failure, preserving original errors and all secondary causes.
    pub fn scope(&self) -> &Error {
        &self.scope
    }
    /// Shutdown failure; may report failed components or a failed process owner.
    pub fn shutdown(&self) -> &Error {
        &self.shutdown
    }
    /// Recovers ownership of both errors without discarding either cause.
    pub fn into_parts(self) -> (Error, Error) {
        (self.scope, self.shutdown)
    }
}

impl fmt::Display for RunFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}; shutdown also failed: {}", self.scope, self.shutdown)
    }
}

impl std::error::Error for RunFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.scope)
    }
}

#[cfg(test)]
#[path = "run_error_test.rs"]
mod run_error_test;
