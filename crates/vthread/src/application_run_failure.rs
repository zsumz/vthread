//! Caller-owned application, structured scope, and explicit shutdown failures.

use super::ScopeRunError;
use crate::Error;
use std::fmt;

/// Failure of [`crate::try_run`], preserving every independent cause.
///
/// At least one field is present. The body error has no `Send`, formatting, or
/// `'static` requirement, and no runtime snapshot retains or formats it. Runtime
/// construction/admission failures appear in `scope`; `shutdown` is absent if no
/// runtime was constructed. Accessors and `into_parts` do not format any error.
/// Body panics unwind instead of creating this value; runtime Drop attempts shutdown
/// during unwind, without a returned shutdown-error channel.
#[derive(Debug)]
pub struct ApplicationRunFailure<E> {
    body: Option<E>,
    scope: Option<Box<Error>>,
    shutdown: Option<Box<Error>>,
}

impl<E> ApplicationRunFailure<E> {
    pub(crate) fn new(scope: Option<ScopeRunError<E>>, shutdown: Option<Error>) -> Self {
        let (body, scope) = match scope {
            None => (None, None),
            Some(ScopeRunError::Body(body)) => (Some(body), None),
            Some(ScopeRunError::Runtime(scope)) => (None, Some(scope)),
            Some(ScopeRunError::BodyAndRuntime { body, runtime }) => (Some(body), Some(runtime)),
        };
        Self {
            body,
            scope: scope.map(Box::new),
            shutdown: shutdown.map(Box::new),
        }
    }

    pub(crate) fn runtime(error: Error) -> Self {
        Self::new(Some(ScopeRunError::Runtime(error)), None)
    }

    /// Original application body error, owned solely by this result.
    pub fn body(&self) -> Option<&E> {
        self.body.as_ref()
    }

    /// Construction, admission, inherited policy, or structured child failure.
    /// Aggregated secondary causes remain available through `Error::scope_failure`.
    pub fn scope(&self) -> Option<&Error> {
        self.scope.as_deref()
    }

    /// Explicit runtime shutdown failure, even if the body and scope succeeded.
    pub fn shutdown(&self) -> Option<&Error> {
        self.shutdown.as_deref()
    }

    /// Recovers body, scope/runtime, and shutdown errors without discarding causes.
    pub fn into_parts(self) -> (Option<E>, Option<Error>, Option<Error>) {
        (
            self.body,
            self.scope.map(|error| *error),
            self.shutdown.map(|error| *error),
        )
    }
}

impl<E: fmt::Display> fmt::Display for ApplicationRunFailure<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("application run failed")?;
        if let Some(body) = &self.body {
            write!(f, "; body: {body}")?;
        }
        if let Some(scope) = &self.scope {
            write!(f, "; scope/runtime: {scope}")?;
        }
        if let Some(shutdown) = &self.shutdown {
            write!(f, "; shutdown: {shutdown}")?;
        }
        Ok(())
    }
}

impl<E: std::error::Error + 'static> std::error::Error for ApplicationRunFailure<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Some(body) = &self.body {
            Some(body)
        } else if let Some(scope) = self.scope.as_deref() {
            Some(scope)
        } else {
            self.shutdown.as_deref().map(|error| error as _)
        }
    }
}

#[cfg(test)]
#[path = "application_run_failure_test.rs"]
mod application_run_failure_test;
