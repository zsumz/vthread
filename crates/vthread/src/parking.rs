//! Public one-permit parking and wake handles.
//!
//! ```
//! use std::{rc::Rc, thread, time::Duration};
//! use vthread::{Runtime, parking::{ParkOutcome, park_pair}};
//!
//! fn main() -> vthread::Result<()> {
//!     let runtime = Runtime::builder().carriers(2).build()?;
//!     runtime.run_scope(|scope| {
//!         let (parker, unparker) = park_pair();
//!         let mut waiter = scope.spawn("waiter", move || {
//!             let owner = thread::current().id();
//!             let local = Rc::new(42);
//!             let outcome = parker.park_timeout(Duration::from_secs(5))?;
//!             assert_eq!(thread::current().id(), owner);
//!             Ok::<_, vthread::Error>((*local, outcome))
//!         })?;
//!         thread::spawn(move || unparker.unpark()).join().expect("remote waker");
//!         assert_eq!(waiter.join()??, (42, ParkOutcome::Ready));
//!         Ok(())
//!     })?;
//!     runtime.shutdown().map(|_| ())
//! }
//! ```

use std::{
    fmt,
    time::{Duration, Instant},
};

use crate::{
    Error, Result, context,
    wait::{NotifyResult, WaitBegin, WaitCell, WakeCause},
};

/// The exact selected winner for one parking generation.
///
/// Once a winner is selected, later inherited cancellation or deadline expiry
/// cannot replace it. Policy is observed at the next cooperative boundary.
/// Inherited cancellation selecting first returns [`Error::Cancelled`]; inherited
/// deadline selection returns [`Error::DeadlineExceeded`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParkOutcome {
    /// Readiness or an explicit unpark operation won.
    Ready,
    /// The monotonic deadline won.
    TimedOut,
    /// Explicit [`Unparker::cancel`] won; this is distinct from inherited cancellation.
    Cancelled,
    /// The parking pair was permanently closed.
    Closed,
}

/// The effect of an `Unparker::unpark` call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
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

    pub(crate) fn park_after_checkpoint(
        &self,
        execution: &crate::context::Execution,
    ) -> Result<ParkOutcome> {
        self.park_execution(execution, None, |_, _| Ok(()))
    }

    pub(crate) fn park_registered<G>(
        &self,
        register: impl FnOnce(vthread_stack::ParkToken, &crate::wait::WaitRegistration) -> Result<G>,
    ) -> Result<ParkOutcome> {
        self.park_with(None, register)
    }

    fn park_with<G>(
        &self,
        deadline: Option<Instant>,
        register: impl FnOnce(vthread_stack::ParkToken, &crate::wait::WaitRegistration) -> Result<G>,
    ) -> Result<ParkOutcome> {
        let mounted = context::current().ok_or(Error::OutsideVThread)?;
        let execution = mounted.execution()?;
        execution.data.check()?;
        self.park_execution(execution, deadline, register)
    }

    fn park_execution<G>(
        &self,
        execution: &crate::context::Execution,
        deadline: Option<Instant>,
        register: impl FnOnce(vthread_stack::ParkToken, &crate::wait::WaitRegistration) -> Result<G>,
    ) -> Result<ParkOutcome> {
        let policy = &execution.data;
        let unmasked = policy.masked() == 0;
        let inherited_deadline = policy.deadline().filter(|_| unmasked);
        let inherited_timeout = inherited_deadline
            .is_some_and(|inherited| deadline.is_none_or(|explicit| inherited <= explicit));
        let deadline = deadline.into_iter().chain(inherited_deadline).min();
        match self.wait.begin(execution.id, execution.hub(), deadline)? {
            WaitBegin::Immediate(cause) => selected(cause, inherited_timeout),
            WaitBegin::Park(request) => {
                let token = request.token();
                let mut generation = self.wait.guard(token);
                let registration = self.wait.registration();
                let _subscription = if unmasked {
                    match policy.cancellation().register(token, &registration) {
                        Ok(subscription) => Some(subscription),
                        Err(error) => {
                            self.wait.rollback(token);
                            return Err(error);
                        }
                    }
                } else {
                    None
                };
                let _external = register(token, &registration)?;
                let _publication = execution.publish_wait(token, registration)?;
                let suspension = vthread_stack::Suspension::Park(request);
                if let Err(error) = vthread_stack::suspend(suspension) {
                    self.wait.rollback(token);
                    return Err(Error::from(error));
                }
                let cause = self.wait.finish(token)?;
                generation.disarm();
                selected(cause, inherited_timeout)
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

fn selected(cause: WakeCause, inherited_timeout: bool) -> Result<ParkOutcome> {
    match cause {
        WakeCause::Ready => Ok(ParkOutcome::Ready),
        WakeCause::TimedOut if inherited_timeout => Err(Error::DeadlineExceeded),
        WakeCause::TimedOut => Ok(ParkOutcome::TimedOut),
        WakeCause::Cancelled => Ok(ParkOutcome::Cancelled),
        WakeCause::InheritedCancelled => Err(Error::Cancelled),
        WakeCause::Closed => Ok(ParkOutcome::Closed),
    }
}

#[cfg(test)]
#[path = "parking_test.rs"]
mod parking_test;

#[cfg(test)]
#[path = "parking_deadline_test.rs"]
mod parking_deadline_test;
