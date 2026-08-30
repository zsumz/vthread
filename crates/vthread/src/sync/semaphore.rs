//! Fixed-capacity, FIFO virtual semaphores.

use super::gate::Gate;
use crate::{Error, Result, SuspensionReason};

/// A fixed pool of permits with FIFO selection and bounded outstanding waits.
pub struct Semaphore {
    pub(super) gate: Gate,
}

/// One permit; dropping it returns capacity even during task reclamation.
#[must_use = "dropping a permit immediately returns its capacity"]
pub struct Permit<'a> {
    semaphore: &'a Semaphore,
}

impl Semaphore {
    /// Creates a positive fixed permit pool and positive waiter limit.
    pub fn new(permits: usize, wait_capacity: usize) -> Result<Self> {
        if permits == 0 {
            return Err(Error::invalid_configuration("permits", "must be positive"));
        }
        Ok(Self {
            gate: Gate::new(permits, permits, wait_capacity)?,
        })
    }

    /// Acquires one permit, parking the virtual caller when necessary.
    pub fn acquire(&self) -> Result<Permit<'_>> {
        self.acquire_for(SuspensionReason::Semaphore)
    }

    pub(super) fn acquire_for(&self, reason: SuspensionReason) -> Result<Permit<'_>> {
        self.gate.take(reason)?;
        Ok(Permit { semaphore: self })
    }

    /// Acquires immediately without bypassing previously selected waiters.
    pub fn try_acquire(&self) -> Result<Permit<'_>> {
        self.gate.try_take()?;
        Ok(Permit { semaphore: self })
    }

    /// Stops acquisitions and wakes waiters with `Error::Closed`.
    /// Existing permits remain valid until dropped.
    pub fn close(&self) {
        self.gate.close();
    }
    /// Whether new acquisitions are closed.
    pub fn is_closed(&self) -> bool {
        self.gate.is_closed()
    }
    /// Unreserved capacity, excluding selected tickets and held permits.
    pub fn available_permits(&self) -> usize {
        self.gate.available()
    }
    /// Outstanding wait tickets, including selected but unconsumed permits.
    pub fn waiting(&self) -> usize {
        self.gate.waiting()
    }
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        self.semaphore.gate.signal();
    }
}

#[cfg(test)]
#[path = "semaphore_test.rs"]
mod semaphore_test;
