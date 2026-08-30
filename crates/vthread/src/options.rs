//! Scope policy and inherited task execution metadata.

use crate::{CancellationToken, Error, Result};
use std::time::Instant;

/// Optional policy for a root scope or supervisor.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScopeOptions {
    pub(crate) deadline: Option<Instant>,
}

impl ScopeOptions {
    /// Sets an absolute monotonic deadline checked at runtime boundaries.
    pub fn deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TaskOptions {
    pub(crate) cancellation: CancellationToken,
    pub(crate) deadline: Option<Instant>,
}

impl TaskOptions {
    pub(crate) fn root(options: ScopeOptions, capacity: usize) -> Self {
        Self {
            cancellation: CancellationToken::root(capacity),
            deadline: options.deadline,
        }
    }

    pub(crate) fn child(&self, deadline: Option<Instant>) -> Self {
        Self {
            cancellation: self.cancellation.child_token(),
            deadline: self.deadline.into_iter().chain(deadline).min(),
        }
    }

    pub(crate) fn check(&self) -> Result<()> {
        if self.cancellation.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if self
            .deadline
            .is_some_and(|deadline| deadline <= Instant::now())
        {
            return Err(Error::DeadlineExceeded);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "options_test.rs"]
mod options_test;
