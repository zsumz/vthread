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
#[must_use = "observe the result or explicitly drop it; the scope still owns the task"]
pub struct JoinHandle<T> {
    shared: Arc<Shared>,
    id: TaskId,
    name: Arc<str>,
    cell: Arc<Mutex<JoinCell<T>>>,
    record: SharedTaskRecord,
    taken: bool,
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
            taken: false,
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

    /// Waits without consuming observation ownership. Cancellation/deadlines are retryable.
    /// A completed handle returns immediately, even after its result was consumed or
    /// its diagnostic record was evicted, without checking the caller's cancellation.
    pub fn wait(&mut self) -> Result<()> {
        if self.is_finished() {
            return Ok(());
        }
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
        Ok(())
    }

    /// Waits for reclamation and takes the result once; interruption retains the handle.
    pub fn join(&mut self) -> Result<T> {
        if self.taken {
            return Err(Error::ResultAlreadyTaken);
        }
        self.wait()?;
        self.take_result()
    }

    /// Takes a finished result without waiting or checking the caller's cancellation.
    /// Returns WouldBlock while unfinished, or ResultAlreadyTaken after consumption.
    pub fn take_result(&mut self) -> Result<T> {
        if self.taken {
            return Err(Error::ResultAlreadyTaken);
        }
        if !self.is_finished() {
            return Err(Error::WouldBlock);
        }
        self.taken = true;
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
        let outcome = lock(&self.cell).outcome.take().ok_or(Error::fault(
            crate::error::FaultComponent::Scheduler,
            "completed task has no join outcome",
        ))?;
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
