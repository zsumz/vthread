//! Safe exclusive value ownership across virtual suspension.

use super::{Permit, Semaphore};
use crate::{Result, SuspensionReason, signal::lock};
use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::Mutex as NativeMutex,
};

/// A FIFO virtual mutex that moves its protected value into the active guard.
///
/// No native lock is held while user code runs or suspends. This mutex does not
/// poison: unwinding returns the possibly modified value. Repair application
/// invariants explicitly if a panic can leave them incomplete. It is not reentrant.
pub struct Mutex<T> {
    value: NativeMutex<Option<T>>,
    semaphore: Semaphore,
}

/// Exclusive access that stays on the current carrier and unlocks on drop.
/// Forgetting a guard leaks the value and permanently retains the lock.
///
/// ```compile_fail
/// let mutex = vthread::sync::Mutex::new(42, 1).unwrap();
/// let guard = mutex.try_lock().unwrap();
/// std::thread::scope(|scope| { scope.spawn(move || drop(guard)); });
/// ```
#[must_use = "dropping the guard immediately unlocks the mutex"]
pub struct MutexGuard<'a, T> {
    pub(super) mutex: &'a Mutex<T>,
    value: Option<T>,
    _permit: Permit<'a>,
    _affine: PhantomData<Rc<()>>,
}

impl<T> Mutex<T> {
    /// Creates an unlocked mutex with an explicit positive waiter limit.
    pub fn new(value: T, wait_capacity: usize) -> Result<Self> {
        Ok(Self {
            value: NativeMutex::new(Some(value)),
            semaphore: Semaphore::new(1, wait_capacity)?,
        })
    }

    /// Locks the value, parking the current virtual thread under contention.
    pub fn lock(&self) -> Result<MutexGuard<'_, T>> {
        let permit = self.semaphore.acquire_for(SuspensionReason::Mutex)?;
        Ok(self.guard(permit))
    }

    /// Locks immediately or returns `Error::WouldBlock`; also usable by OS callers.
    pub fn try_lock(&self) -> Result<MutexGuard<'_, T>> {
        Ok(self.guard(self.semaphore.try_acquire()?))
    }

    fn guard<'a>(&'a self, permit: Permit<'a>) -> MutexGuard<'a, T> {
        let value = lock(&self.value)
            .take()
            .expect("mutex permit owns its value");
        MutexGuard {
            mutex: self,
            value: Some(value),
            _permit: permit,
            _affine: PhantomData,
        }
    }

    /// Number of outstanding lock waits, including selected waiters.
    pub fn waiting(&self) -> usize {
        self.semaphore.waiting()
    }
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value.as_ref().expect("live mutex guard")
    }
}
impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value.as_mut().expect("live mutex guard")
    }
}
impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        // Restore the value before the permit field wakes the next owner.
        *lock(&self.mutex.value) = self.value.take();
    }
}

#[cfg(test)]
#[path = "mutex_test.rs"]
mod mutex_test;
