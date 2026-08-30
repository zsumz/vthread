//! Typed results for borrowed children that cannot escape their lexical owner.

use crate::{
    Error, Result, SuspensionReason, join::JoinCell, join_wait, signal::lock,
    task::SharedTaskRecord,
};
use std::{cell::RefCell, marker::PhantomData, rc::Rc};

/// A non-Send borrowed child's result, usable only inside its local scope.
///
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<vthread::LocalJoinHandle<'static, usize>>();
/// ```
pub struct LocalJoinHandle<'scope, T> {
    pub(crate) record: SharedTaskRecord,
    pub(crate) cell: Rc<RefCell<JoinCell<T>>>,
    pub(crate) lifetime: PhantomData<&'scope mut &'scope ()>,
}

impl<T> LocalJoinHandle<'_, T> {
    /// Returns the child's identity.
    pub fn task_id(&self) -> crate::TaskId {
        lock(&self.record).id
    }
    /// Whether the owner has reclaimed the child stack.
    pub fn is_finished(&self) -> bool {
        lock(&self.record).completion.done()
    }
    /// Parks the caller until the child stack is reclaimed, then returns its result.
    pub fn join(self) -> Result<T> {
        join_wait::wait_for(&self.record, SuspensionReason::Join(self.task_id()), false)?;
        let mut record = lock(&self.record);
        record.outcome_observed = true;
        if let Some(reason) = record.failure {
            return Err(Error::TaskAborted {
                task: record.id,
                reason,
            });
        }
        if let Some(panic) = record.panic.clone() {
            return Err(Error::task_panicked(
                record.id,
                record.name.to_string(),
                panic,
            ));
        }
        let id = record.id;
        let name = record.name.to_string();
        drop(record);
        self.cell
            .borrow_mut()
            .outcome
            .take()
            .ok_or(Error::Invariant("local child has no outcome"))?
            .map_err(|panic| Error::task_panicked(id, name, panic))
    }
}

#[cfg(test)]
#[path = "local_join_test.rs"]
mod local_join_test;
