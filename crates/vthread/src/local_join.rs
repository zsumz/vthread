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
#[must_use = "observe the result or explicitly drop it; the local scope still owns the child"]
pub struct LocalJoinHandle<'scope, T> {
    pub(crate) record: SharedTaskRecord,
    pub(crate) cell: Rc<RefCell<JoinCell<T>>>,
    pub(crate) lifetime: PhantomData<&'scope mut &'scope ()>,
    pub(crate) taken: bool,
}

impl<T> LocalJoinHandle<'_, T> {
    /// Returns the child's identity.
    pub fn task_id(&self) -> crate::TaskId {
        lock(&self.record).id
    }
    /// Returns this child's token; its parent and siblings are unaffected by cancellation.
    pub fn cancellation_token(&self) -> crate::CancellationToken {
        lock(&self.record).options.cancellation.clone()
    }
    /// Requests cooperative cancellation while preserving child ownership and its result.
    pub fn cancel(&self) {
        self.cancellation_token().cancel();
    }
    /// Whether the owner has reclaimed the child stack.
    pub fn is_finished(&self) -> bool {
        lock(&self.record).completion.done()
    }
    /// Parks until reclamation without consuming observation ownership on interruption.
    /// Completed children succeed immediately and idempotently, even after result
    /// consumption or later caller cancellation/deadline expiry. Policy governs only
    /// waiting for an incomplete child.
    /// While parked, completion selecting first commits the wait; later policy is
    /// observed at the next cooperative boundary. Policy selecting first interrupts it.
    pub fn wait(&mut self) -> Result<()> {
        if self.is_finished() {
            return Ok(());
        }
        join_wait::wait_for(&self.record, SuspensionReason::Join(self.task_id()), false)?;
        Ok(())
    }

    /// Waits and takes the result once; cancellation/deadlines leave the handle intact.
    pub fn join(&mut self) -> Result<T> {
        if self.taken {
            return Err(Error::ResultAlreadyTaken);
        }
        self.wait()?;
        self.take_result()
    }

    /// Takes an already reclaimed child's result without a suspension boundary.
    /// Returns WouldBlock while unfinished, or ResultAlreadyTaken after consumption.
    pub fn take_result(&mut self) -> Result<T> {
        if self.taken {
            return Err(Error::ResultAlreadyTaken);
        }
        if !self.is_finished() {
            return Err(Error::WouldBlock);
        }
        self.taken = true;
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
            .ok_or(Error::fault(
                crate::error::FaultComponent::Scheduler,
                "local child has no outcome",
            ))?
            .map_err(|panic| Error::task_panicked(id, name, panic))
    }
}

#[cfg(test)]
#[path = "local_join_test.rs"]
mod local_join_test;
