//! Public one-permit parking and wake handles.

use std::{
    fmt,
    time::{Duration, Instant},
};

use crate::{
    Error, Result, context,
    wait::{NotifyResult, WaitBegin, WaitCell, WakeCause},
};

/// The selected winner for one parking generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParkOutcome {
    /// Readiness or an explicit unpark operation won.
    Ready,
    /// The monotonic deadline won.
    TimedOut,
    /// Explicit cancellation won.
    Cancelled,
    /// The parking pair was permanently closed.
    Closed,
}

/// The effect of an `Unparker::unpark` call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnparkResult {
    /// An active parked task was selected for wakeup.
    Woke,
    /// A single permit was stored for the next park operation.
    Stored,
    /// The pair was already closed.
    Closed,
}

/// The single-consumer side of a bounded one-permit wake primitive.
pub struct Parker {
    pub(crate) wait: WaitCell,
}

impl Parker {
    /// Parks the current virtual thread until readiness, cancellation, or close.
    pub fn park(&self) -> Result<ParkOutcome> {
        self.park_deadline(None)
    }

    /// Parks until a relative monotonic timeout expires.
    pub fn park_timeout(&self, timeout: Duration) -> Result<ParkOutcome> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(Error::DeadlineOverflow)?;
        self.park_deadline(Some(deadline))
    }

    /// Parks until an absolute monotonic deadline.
    pub fn park_until(&self, deadline: Instant) -> Result<ParkOutcome> {
        self.park_deadline(Some(deadline))
    }

    fn park_deadline(&self, deadline: Option<Instant>) -> Result<ParkOutcome> {
        self.park_with(deadline, |_, _| Ok(()))
    }

    pub(crate) fn park_registered<G>(
        &self,
        register: impl FnOnce(vthread_stack::ParkToken, crate::wait::WaitRegistration) -> Result<G>,
    ) -> Result<ParkOutcome> {
        self.park_with(None, register)
    }

    fn park_with<G>(
        &self,
        deadline: Option<Instant>,
        register: impl FnOnce(vthread_stack::ParkToken, crate::wait::WaitRegistration) -> Result<G>,
    ) -> Result<ParkOutcome> {
        let mounted = context::current().ok_or(Error::OutsideVThread)?;
        let execution = mounted.execution()?;
        execution.data.check()?;
        let policy = &execution.data.options;
        let unmasked = execution.data.masked.get() == 0;
        let deadline = deadline
            .into_iter()
            .chain(policy.deadline.filter(|_| unmasked))
            .min();
        let hub = mounted.hub();
        match self.wait.begin(mounted.task_id(), &hub, deadline)? {
            WaitBegin::Immediate(cause) => Ok(ParkOutcome::from(cause)),
            WaitBegin::Park(request) => {
                let token = request.token();
                let _generation = self.wait.guard(token);
                let _subscription = if unmasked {
                    match policy
                        .cancellation
                        .register(token, self.wait.registration())
                    {
                        Ok(subscription) => Some(subscription),
                        Err(error) => {
                            self.wait.rollback(token);
                            return Err(error);
                        }
                    }
                } else {
                    None
                };
                let _external = register(token, self.wait.registration())?;
                let suspension = vthread_stack::Suspension::Park(request);
                if let Err(error) = vthread_stack::suspend(suspension) {
                    self.wait.rollback(token);
                    return Err(Error::from(error));
                }
                let cause = self.wait.finish(token)?;
                execution.data.check()?;
                Ok(ParkOutcome::from(cause))
            }
        }
    }
}

impl fmt::Debug for Parker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Parker").finish_non_exhaustive()
    }
}

/// Cloneable control side of a parking pair.
#[derive(Clone)]
pub struct Unparker {
    wait: WaitCell,
}

impl Unparker {
    /// Wakes the active generation or stores one future permit.
    pub fn unpark(&self) -> UnparkResult {
        match self.wait.notify() {
            NotifyResult::Woke => UnparkResult::Woke,
            NotifyResult::Stored => UnparkResult::Stored,
            NotifyResult::Closed => UnparkResult::Closed,
        }
    }

    /// Cancels the active generation without closing the pair.
    pub fn cancel(&self) -> bool {
        self.wait.cancel()
    }

    /// Permanently closes the pair and wakes an active generation.
    pub fn close(&self) -> bool {
        self.wait.close()
    }

    /// Returns whether the pair is permanently closed.
    pub fn is_closed(&self) -> bool {
        self.wait.is_closed()
    }
}

impl fmt::Debug for Unparker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Unparker")
            .field("closed", &self.is_closed())
            .finish_non_exhaustive()
    }
}

/// Creates a single-consumer parker and cloneable wake handle.
pub fn park_pair() -> (Parker, Unparker) {
    let wait = WaitCell::new();
    (Parker { wait: wait.clone() }, Unparker { wait })
}

impl From<WakeCause> for ParkOutcome {
    fn from(cause: WakeCause) -> Self {
        match cause {
            WakeCause::Ready => Self::Ready,
            WakeCause::TimedOut => Self::TimedOut,
            WakeCause::Cancelled => Self::Cancelled,
            WakeCause::Closed => Self::Closed,
        }
    }
}

#[cfg(test)]
#[path = "parking_test.rs"]
mod parking_test;
