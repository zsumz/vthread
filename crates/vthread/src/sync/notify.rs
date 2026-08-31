//! FIFO notifications with one stored permit and bounded waiter reservations.

use super::gate::Gate;
use crate::{Result, SuspensionReason};

/// A notification source. `notify_one` selects the oldest unselected waiter, or
/// stores one permit if none exists. A cancelled selection passes to the next
/// waiter or returns to that single stored permit.
pub struct Notify {
    gate: Gate,
}

impl Notify {
    /// Creates a notification source using [`super::DEFAULT_WAIT_CAPACITY`].
    pub fn new() -> Self {
        Self::with_wait_capacity(super::DEFAULT_WAIT_CAPACITY)
            .expect("default waiter capacity is positive")
    }

    /// Creates a notification source with a positive outstanding-wait limit.
    pub fn with_wait_capacity(wait_capacity: usize) -> Result<Self> {
        Ok(Self {
            gate: Gate::new(0, 1, wait_capacity)?,
        })
    }
    /// Consumes a notification or parks the current virtual thread.
    pub fn notified(&self) -> Result<()> {
        self.gate.take(SuspensionReason::Notify)
    }
    /// Consumes a stored permit immediately; works from an OS caller too.
    pub fn try_notified(&self) -> Result<()> {
        self.gate.try_take()
    }
    /// Selects one FIFO waiter or stores a single future permit.
    pub fn notify_one(&self) {
        self.gate.signal();
    }
    /// Selects all current waiters without storing new permits for future waits.
    /// A cancelled broadcast selection does not notify a later waiter.
    pub fn notify_waiters(&self) {
        self.gate.broadcast();
    }
    /// Permanently closes this source and fails outstanding waits.
    pub fn close(&self) {
        self.gate.close();
    }
    /// Whether the source is closed.
    pub fn is_closed(&self) -> bool {
        self.gate.is_closed()
    }
    /// Outstanding wait tickets, including selected but unconsumed notifications.
    pub fn waiting(&self) -> usize {
        self.gate.waiting()
    }
    /// Configured outstanding-wait limit, including selected waiters.
    pub fn wait_capacity(&self) -> usize {
        self.gate.wait_capacity()
    }
}

impl Default for Notify {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "notify_test.rs"]
mod notify_test;
