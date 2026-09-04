//! Private exclusive-value backend for `vthread`; not an independent public API.
//!
//! The safe runtime owns queueing and transfers the linear [`Ownership`] capability. This crate
//! contains only the unsafe dereference boundary and the atomic ownership state that proves at
//! most one live [`ExclusiveGuard`] can access a value.

#![deny(unsafe_op_in_unsafe_fn)]

mod exclusive;

pub use exclusive::{ExclusiveCell, ExclusiveGuard, Ownership, OwnershipSlot, QueueDecision};

#[cfg(test)]
#[path = "lib_test.rs"]
mod lib_test;
