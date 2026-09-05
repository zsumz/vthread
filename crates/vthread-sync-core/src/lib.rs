//! Private synchronization kernels for `vthread`; not an independent public API.
//!
//! The safe runtime owns queueing and transfers the linear [`Ownership`] capability. This crate
//! isolates the unsafe dereference boundary and its atomic ownership proof. The experimental
//! [`WakeMailbox`] models bounded wake publication separately from scheduling and task lifetime.

#![deny(unsafe_op_in_unsafe_fn)]

mod exclusive;
mod spin_mutex;
mod wake_atomic;
mod wake_mailbox;

pub use exclusive::{ExclusiveCell, ExclusiveGuard, Ownership, OwnershipSlot, QueueDecision};
pub use spin_mutex::{SpinMutex, SpinMutexGuard};
pub use wake_mailbox::WakeMailbox;

#[cfg(test)]
#[path = "lib_test.rs"]
mod lib_test;
