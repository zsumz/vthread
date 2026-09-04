//! Safe exclusive value ownership across virtual suspension.

use super::{
    mutex_queue::{MutexQueue, Subscription},
    wait::Wait,
};
use crate::{Error, Result, SuspensionReason};
use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
    rc::Rc,
};
use vthread_sync_core::{ExclusiveCell, ExclusiveGuard};

/// A bounded FIFO virtual mutex with direct ownership handoff.
///
/// The protected value is accessed only through a linear capability. Contended unlock keeps the
/// value logically locked while transferring that capability to the selected task. This mutex
/// does not expose poisoning: unwinding releases the possibly modified value. Repair application
/// invariants explicitly if a panic can leave them incomplete. It is not reentrant.
pub struct Mutex<T> {
    value: ExclusiveCell<T>,
    queue: MutexQueue,
}

/// Exclusive access that stays on the current carrier and unlocks on drop.
/// Forgetting a guard leaks the value and permanently retains the lock.
///
/// ```compile_fail
/// let mutex = vthread::sync::Mutex::new(42);
/// let guard = mutex.try_lock().unwrap();
/// std::thread::scope(|scope| { scope.spawn(move || drop(guard)); });
/// ```
#[must_use = "dropping the guard immediately unlocks the mutex"]
pub struct MutexGuard<'a, T> {
    pub(super) mutex: &'a Mutex<T>,
    value: Option<ExclusiveGuard<'a, T>>,
    _affine: PhantomData<Rc<()>>,
}

impl<T> Mutex<T> {
    /// Creates an unlocked mutex using [`super::DEFAULT_WAIT_CAPACITY`].
    pub fn new(value: T) -> Self {
        Self::with_wait_capacity(value, super::DEFAULT_WAIT_CAPACITY)
            .expect("default waiter capacity is positive")
    }

    /// Creates an unlocked mutex with an explicit positive waiter limit.
    pub fn with_wait_capacity(value: T, wait_capacity: usize) -> Result<Self> {
        Ok(Self {
            value: ExclusiveCell::new(value),
            queue: MutexQueue::new(wait_capacity)?,
        })
    }

    /// Locks the value, parking the current virtual thread under contention.
    pub fn lock(&self) -> Result<MutexGuard<'_, T>> {
        crate::context::check_current()?;
        if let Some(value) = self.value.try_lock() {
            return Ok(self.guard(value));
        }
        let wait = Wait::enter_after_check(SuspensionReason::Mutex)?;
        let synchronization_wait = wait.synchronization_wait()?;
        let value = match self.queue.subscribe(&self.value, synchronization_wait)? {
            Subscription::Acquired(value) => value,
            Subscription::Waiting(ticket) => {
                let ownership = ticket.wait(&wait)?;
                match self.value.claim(ownership) {
                    Ok(value) => value,
                    Err(ownership) => {
                        self.queue.release_ownership(&self.value, ownership);
                        return Err(Error::fault(
                            crate::error::FaultComponent::Scheduler,
                            "selected mutex ticket lost ownership",
                        ));
                    }
                }
            }
        };
        Ok(self.guard(value))
    }

    /// Locks immediately or returns `Error::WouldBlock`; also usable by OS callers.
    pub fn try_lock(&self) -> Result<MutexGuard<'_, T>> {
        self.value
            .try_lock()
            .map(|value| self.guard(value))
            .ok_or(Error::WouldBlock)
    }

    fn guard<'a>(&'a self, value: ExclusiveGuard<'a, T>) -> MutexGuard<'a, T> {
        MutexGuard {
            mutex: self,
            value: Some(value),
            _affine: PhantomData,
        }
    }

    /// Number of outstanding lock waits, including selected waiters.
    pub fn waiting(&self) -> usize {
        self.queue.waiting()
    }

    /// Configured outstanding-wait limit, including selected waiters.
    pub fn wait_capacity(&self) -> usize {
        self.queue.wait_capacity()
    }
}

impl<T: Default> Default for Mutex<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value.as_deref().expect("live mutex guard")
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value.as_deref_mut().expect("live mutex guard")
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        let value = self.value.take().expect("live mutex guard");
        self.mutex.queue.release(&self.mutex.value, value);
    }
}

#[cfg(test)]
#[path = "mutex_test.rs"]
mod mutex_test;
