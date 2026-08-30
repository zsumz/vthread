//! Typed task completion handles.

use std::{
    fmt,
    sync::{Arc, Mutex},
};

use crate::{
    Error, PanicReport, Result, TaskId, context, control::Shared, signal::lock,
    task::SharedTaskRecord,
};

pub(crate) struct JoinCell<T> {
    pub(crate) outcome: Option<std::result::Result<T, PanicReport>>,
}

/// A typed result handle whose completion follows carrier-side stack reclamation.
pub struct JoinHandle<T> {
    shared: Arc<Shared>,
    id: TaskId,
    name: Arc<str>,
    cell: Arc<Mutex<JoinCell<T>>>,
    record: SharedTaskRecord,
}

impl<T> JoinHandle<T> {
    pub(crate) fn new(
        shared: Arc<Shared>,
        id: TaskId,
        name: Arc<str>,
        cell: Arc<Mutex<JoinCell<T>>>,
        record: SharedTaskRecord,
    ) -> Self {
        Self {
            shared,
            id,
            name,
            cell,
            record,
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

    /// Parks a virtual caller or blocks an OS caller until stack and context reclamation.
    pub fn join(self) -> Result<T> {
        if context::current().is_some() {
            crate::join_wait::wait_for(
                &self.record,
                crate::SuspensionReason::Join(self.id),
                false,
            )?;
        } else {
            let scope = lock(&self.record).scope;
            self.shared.wait(scope, Some(self.id))?;
        }
        let (failure, panic) = {
            let mut record = lock(&self.record);
            record.outcome_observed = true;
            (record.failure, record.panic.clone())
        };
        if let Some(reason) = failure {
            return Err(Error::TaskAborted {
                task: self.id,
                reason,
            });
        }
        if let Some(panic) = panic {
            return Err(Error::task_panicked(self.id, self.name.to_string(), panic));
        }
        let outcome = lock(&self.cell)
            .outcome
            .take()
            .ok_or(Error::Invariant("completed task has no join outcome"))?;
        outcome.map_err(|panic| Error::task_panicked(self.id, self.name.to_string(), panic))
    }
}

impl<T> fmt::Debug for JoinHandle<T> {
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
