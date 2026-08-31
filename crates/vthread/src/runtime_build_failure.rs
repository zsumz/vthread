//! Caller-owned causes from runtime initialization and its explicit rollback.

use crate::Error;
use std::fmt;

/// Both partial runtime construction and its shutdown failed.
///
/// Builders return the original construction error directly when rollback succeeds.
/// This bounded pair retains both causes when rollback fails; it does not imply that
/// uncertain resources were released. A lifecycle failure still requires process restart.
#[derive(Debug)]
pub struct RuntimeBuildFailure {
    construction: Error,
    shutdown: Error,
}

impl RuntimeBuildFailure {
    pub(crate) fn new(construction: Error, shutdown: Error) -> Self {
        Self {
            construction,
            shutdown,
        }
    }
    /// The original initialization failure, including its operating-system source.
    pub fn construction(&self) -> &Error {
        &self.construction
    }
    /// Cleanup failure, including component reports or lifecycle fail-stop.
    pub fn shutdown(&self) -> &Error {
        &self.shutdown
    }
    /// Recovers both original causes without formatting or discarding either one.
    pub fn into_parts(self) -> (Error, Error) {
        (self.construction, self.shutdown)
    }
}

impl fmt::Display for RuntimeBuildFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "construction failed: {}; shutdown also failed: {}",
            self.construction, self.shutdown
        )
    }
}

impl std::error::Error for RuntimeBuildFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.construction)
    }
}

#[cfg(test)]
#[path = "runtime_build_failure_test.rs"]
mod runtime_build_failure_test;
