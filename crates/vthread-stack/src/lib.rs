//! Private stack and context-switching backend for `vthread`.

#![deny(unsafe_op_in_unsafe_fn)]

mod fiber;
mod pool;
mod scoped;

pub use fiber::{Fiber, FiberState, ParkRequest, ParkToken, SuspendError, Suspension, suspend};
pub use pool::{StackPool, StackPoolSnapshot};
pub use scoped::{FiberLease, FiberScope, fiber_scope};

#[cfg(test)]
#[path = "lib_test.rs"]
mod lib_test;
