//! Versioned route notifications without an owner RMW on the publication word.

use super::wake_atomic::{AtomicU64, Ordering};

const SLEEPING: u64 = 1 << 63;
const ROUTE_MASK: u64 = !SLEEPING;

#[repr(align(64))]
struct Published(AtomicU64);

#[repr(align(64))]
struct Owner {
    seen: AtomicU64,
    batch: AtomicU64,
}

/// An experimental bounded mailbox, not the runtime's current wake queue.
///
/// Each route has **at most one outstanding publication**, including an in-progress
/// payload read. The caller must prevent republication until the sole consumer has
/// copied that payload and released its route reservation. A wake-generation winner
/// must also exclude simultaneous publishers of the same route.
///
/// Only `publish` may run on producer threads. All other operations belong to one
/// consumer. Owner fields are atomic solely to allow sharing the mailbox; their
/// relaxed accesses do not make multiple consumers valid. Route identity, payload
/// storage, stale-generation rejection, and the actual sleep primitive are external.
pub struct WakeMailbox {
    published: Published,
    owner: Owner,
}

impl WakeMailbox {
    /// Number of routes represented without an overflow queue.
    pub const ROUTES: usize = 63;

    /// Creates an empty, unarmed mailbox.
    pub fn new() -> Self {
        Self {
            published: Published(AtomicU64::new(0)),
            owner: Owner {
                seen: AtomicU64::new(0),
                batch: AtomicU64::new(0),
            },
        }
    }

    /// Publishes prior payload writes, returning whether the owner needs notification.
    ///
    /// The caller must hold this route's exclusive publication reservation. After
    /// publication it must not change the payload until the consumer releases it.
    /// Panics if `route` is outside `0..Self::ROUTES`.
    pub fn publish(&self, route: usize) -> bool {
        assert!(route < Self::ROUTES, "mailbox route out of bounds");
        // Discarding the old word lets x86 use one locked bitwise instruction
        // instead of synthesizing a value-returning XOR with a CAS retry loop.
        self.published.0.fetch_xor(1 << route, Ordering::Release);
        // Acquire the arming Release before checking the caller's separate waiter
        // registration. A later false observation is safe: disarming leaves the
        // owner awake, and rearming cannot miss an unacknowledged publication.
        self.published.0.load(Ordering::Acquire) & SLEEPING != 0
    }

    /// Acknowledges one route; the caller must copy its payload before releasing it.
    ///
    /// A captured batch drains before fresh publications, preventing repeated reuse
    /// of a low route from starving an older high route. This is not publication FIFO.
    pub fn pop(&self) -> Option<usize> {
        let seen = self.owner.seen.load(Ordering::Relaxed);
        let mut batch = self.owner.batch.load(Ordering::Relaxed);
        if batch == 0 {
            batch = (self.published.0.load(Ordering::Acquire) ^ seen) & ROUTE_MASK;
            if batch == 0 {
                return None;
            }
        }
        let route = batch.trailing_zeros() as usize;
        let bit = 1 << route;
        self.owner.batch.store(batch ^ bit, Ordering::Relaxed);
        // Acknowledgement precedes the caller's Release of the payload reservation.
        // Otherwise two publications could cancel in the parity word before a read.
        self.owner.seen.store(seen ^ bit, Ordering::Relaxed);
        Some(route)
    }

    /// Checks for published work on the owner without scanning payload slots.
    pub fn has_pending(&self) -> bool {
        (self.published.0.load(Ordering::Acquire) ^ self.owner.seen.load(Ordering::Relaxed))
            & ROUTE_MASK
            != 0
    }

    /// Returns true for pending work, or atomically arms an empty mailbox for sleep.
    ///
    /// A racing producer either changes the compared word (so the owner retries),
    /// or observes the sleeping bit and requests notification. The caller must have
    /// registered with its sleep primitive before this final readiness check.
    pub fn arm_wait(&self) -> bool {
        let seen = self.owner.seen.load(Ordering::Relaxed);
        let mut word = self.published.0.load(Ordering::Acquire);
        loop {
            if (word ^ seen) & ROUTE_MASK != 0 {
                return true;
            }
            if word & SLEEPING != 0 {
                return false;
            }
            match self.published.0.compare_exchange_weak(
                word,
                word | SLEEPING,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return false,
                Err(observed) => word = observed,
            }
        }
    }

    /// Clears sleep intent without erasing a racing route publication.
    pub fn disarm_wait(&self) {
        self.published.0.fetch_and(ROUTE_MASK, Ordering::Release);
    }
}

impl Default for WakeMailbox {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "wake_mailbox_test.rs"]
mod wake_mailbox_test;
