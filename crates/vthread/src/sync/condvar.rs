//! Non-sticky condition notifications registered before predicate-lock release.

use super::{MutexGuard, gate::Gate, wait::Wait};
use crate::{Result, SuspensionReason};

/// A bounded condition variable used with a virtual mutex and a predicate loop.
/// Notifications without waiters are discarded. Always change the predicate
/// while holding the same mutex that the waiter uses.
pub struct Condvar {
    gate: Gate,
}

impl Condvar {
    /// Creates a condition variable using [`super::DEFAULT_WAIT_CAPACITY`].
    pub fn new() -> Self {
        Self::with_wait_capacity(super::DEFAULT_WAIT_CAPACITY)
            .expect("default waiter capacity is positive")
    }

    /// Creates a condition variable with a positive outstanding-wait limit.
    pub fn with_wait_capacity(wait_capacity: usize) -> Result<Self> {
        Ok(Self {
            gate: Gate::new(0, 0, wait_capacity)?,
        })
    }
    /// Registers, unlocks, waits, then reacquires the predicate mutex.
    ///
    /// On any error (including cancellation, deadline, or waiter capacity), the
    /// supplied guard is dropped and the mutex is left unlocked. Reacquisition
    /// itself obeys task policy and can fail. Notifications do not assert that
    /// the predicate is true; callers must loop and recheck it.
    pub fn wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> Result<MutexGuard<'a, T>> {
        let wait = Wait::enter(SuspensionReason::Condvar)?;
        let ticket = self.gate.subscribe(&wait)?;
        let mutex = guard.mutex;
        drop(guard);
        ticket.wait(&wait)?;
        mutex.lock()
    }
    /// Selects the oldest unselected waiter, if any.
    pub fn notify_one(&self) {
        self.gate.signal();
    }
    /// Selects all currently registered waiters without storing future permits.
    pub fn notify_all(&self) {
        self.gate.broadcast();
    }
    /// Permanently closes this variable and fails outstanding waits.
    pub fn close(&self) {
        self.gate.close();
    }
    /// Outstanding condition waits, including selected waiters.
    pub fn waiting(&self) -> usize {
        self.gate.waiting()
    }
    /// Configured outstanding-wait limit, including selected waiters.
    pub fn wait_capacity(&self) -> usize {
        self.gate.wait_capacity()
    }
}

impl Default for Condvar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "condvar_test.rs"]
mod condvar_test;
