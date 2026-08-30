//! Typed task completion handles.

use std::{
    fmt,
    marker::PhantomData,
    sync::{Arc, Mutex},
};

use crate::{
    Error, PanicReport, Result, Runtime, TaskId, context, signal::lock, task::SharedTaskRecord,
};

pub(crate) struct JoinCell<T> {
    pub(crate) outcome: Option<std::result::Result<T, PanicReport>>,
}

/// A typed result handle whose completion follows carrier-side stack reclamation.
pub struct JoinHandle<'scope, T> {
    runtime: &'scope Runtime,
    id: TaskId,
    name: Arc<str>,
    cell: Arc<Mutex<JoinCell<T>>>,
    record: SharedTaskRecord,
    _invariant: PhantomData<&'scope mut &'scope ()>,
}

impl<'scope, T> JoinHandle<'scope, T> {
    pub(crate) fn new(
        runtime: &'scope Runtime,
        id: TaskId,
        name: Arc<str>,
        cell: Arc<Mutex<JoinCell<T>>>,
        record: SharedTaskRecord,
    ) -> Self {
        Self {
            runtime,
            id,
            name,
            cell,
            record,
            _invariant: PhantomData,
        }
    }

    /// Returns the task identity.
    pub fn task_id(&self) -> TaskId {
        self.id
    }

    /// Returns whether the task has reached a terminal state.
    pub fn is_finished(&self) -> bool {
        lock(&self.record).status.is_terminal()
    }

    /// Waits on the calling OS thread until the owning carrier reclaims the child stack.
    pub fn join(self) -> Result<T> {
        if context::current().is_some() {
            return Err(Error::InsideVThread);
        }
        let scope = lock(&self.record).scope;
        let waited = self.runtime.shared.wait(scope, Some(self.id));
        let failure = {
            let mut record = lock(&self.record);
            record.outcome_observed = true;
            record.failure
        };
        waited?;
        if let Some(reason) = failure {
            return Err(Error::TaskAborted {
                task: self.id,
                reason,
            });
        }
        let outcome = lock(&self.cell)
            .outcome
            .take()
            .ok_or(Error::Invariant("completed task has no join outcome"))?;
        outcome.map_err(|panic| Error::task_panicked(self.id, self.name.to_string(), panic))
    }
}

impl<T> fmt::Debug for JoinHandle<'_, T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JoinHandle")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("status", &lock(&self.record).status)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "join_test.rs"]
mod join_test;
