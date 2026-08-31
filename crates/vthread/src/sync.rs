//! Bounded synchronization that parks virtual threads instead of their carriers.
//!
//! Blocking methods require a virtual caller and respect inherited cancellation and
//! deadlines. Try methods, notifications, closing, and guard destruction also work
//! from ordinary threads. Simple constructors use [`DEFAULT_WAIT_CAPACITY`];
//! `with_wait_capacity` constructors set a different positive waiter limit.
//! Native locks protect only short metadata operations and are never held at suspension.

mod condvar;
mod gate;
mod mutex;
mod notify;
mod semaphore;
pub(crate) mod wait;

pub use condvar::Condvar;
pub use mutex::{Mutex, MutexGuard};
pub use notify::Notify;
pub use semaphore::{Permit, Semaphore};

/// Default outstanding-wait limit for each synchronization primitive and each
/// channel direction. Selected but unconsumed wait tickets count toward this limit.
/// Exceeding it returns `Error::Capacity`; primitives never grow an unbounded queue.
pub const DEFAULT_WAIT_CAPACITY: usize = 64;

#[cfg(test)]
#[path = "sync_test.rs"]
mod sync_test;
