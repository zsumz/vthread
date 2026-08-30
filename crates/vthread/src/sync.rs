//! Bounded synchronization that parks virtual threads instead of their carriers.
//!
//! Blocking methods require a virtual caller and respect inherited cancellation and
//! deadlines. Try methods, notifications, closing, and guard destruction also work
//! from ordinary threads. All constructors require an explicit waiter capacity.
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

#[cfg(test)]
#[path = "sync_test.rs"]
mod sync_test;
