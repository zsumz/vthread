//! Private stack and context-switching backend for `vthread`.

#![deny(unsafe_op_in_unsafe_fn)]

mod fiber;
mod pool;

pub use fiber::{Fiber, FiberState, SuspendError, Suspension, suspend};
pub use pool::{StackPool, StackPoolSnapshot};

#[cfg(test)]
#[path = "lib_test.rs"]
mod lib_test;
