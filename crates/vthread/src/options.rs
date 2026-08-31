//! Scope policy and inherited task execution metadata.

use crate::{CancellationToken, Error, Result, TaskId};
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

/// Optional policy for one child; omitted policy inherits its owner and spawning task.
#[derive(Clone, Copy, Debug, Default)]
pub struct SpawnOptions {
    pub(crate) deadline: Option<Instant>,
}

impl SpawnOptions {
    /// Narrows the child's deadline; it cannot extend an owner or caller deadline.
    pub fn deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }
}

pub(crate) struct SpawnParent {
    pub(crate) id: TaskId,
    pub(crate) scope: u64,
    pub(crate) options: TaskOptions,
}

#[derive(Clone, Debug)]
pub(crate) struct TaskOptions {
    pub(crate) cancellation: CancellationToken,
    pub(crate) deadline: Option<Instant>,
}

impl TaskOptions {
    #[cfg(test)]
    pub(crate) fn root(options: ScopeOptions, capacity: usize) -> Self {
        Self {
            cancellation: CancellationToken::root(capacity),
            deadline: options.deadline,
        }
    }

    pub(crate) fn spawned(
        &self,
        scope: u64,
        parent: Option<&SpawnParent>,
        options: SpawnOptions,
    ) -> Self {
        let Some(parent) = parent else {
            return self.child(options.deadline);
        };
        let cancellation = if parent.scope == scope {
            parent.options.cancellation.child_token()
        } else {
            parent
                .options
                .cancellation
                .child_for_scope(&self.cancellation)
        };
        Self {
            cancellation,
            deadline: self
                .deadline
                .into_iter()
                .chain(parent.options.deadline)
                .chain(options.deadline)
                .min(),
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
