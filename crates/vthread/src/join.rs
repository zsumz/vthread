//! Typed task completion handles.

use std::{
    cell::RefCell,
    fmt,
    marker::PhantomData,
    rc::Rc,
};

use crate::{
    Error, PanicReport, Result, Runtime, TaskId, TaskStatus,
    task::SharedTaskRecord,
};

pub(crate) struct JoinCell<T> {
    pub(crate) outcome: Option<std::result::Result<T, PanicReport>>,
}

/// A carrier-local handle to one task result.
pub struct JoinHandle<'scope, T> {
    runtime: &'scope Runtime,
    id: TaskId,
    name: Rc<str>,
    cell: Rc<RefCell<JoinCell<T>>>,
    record: SharedTaskRecord,
    _invariant: PhantomData<&'scope mut &'scope ()>,
}

impl<'scope, T> JoinHandle<'scope, T> {
    pub(crate) fn new(
        runtime: &'scope Runtime,
        id: TaskId,
        name: Rc<str>,
        cell: Rc<RefCell<JoinCell<T>>>,
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
        self.record.borrow().status.is_terminal()
    }

    /// Drives the carrier until this task completes and returns its result.
    pub fn join(self) -> Result<T> {
        self.runtime.run_until(self.id)?;
        self.record.borrow_mut().outcome_observed = true;
        let outcome = self
            .cell
            .borrow_mut()
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
            .field("status", &self.record.borrow().status)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "join_test.rs"]
mod join_test;
