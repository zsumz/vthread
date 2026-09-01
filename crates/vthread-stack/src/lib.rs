//! Private, unsupported stack backend for `vthread`; not an independent public API.
//!
//! Direct downstream use has no compatibility contract. Cleanup mount callbacks are
//! runtime integration hooks and must succeed. A persistent mount failure during lexical
//! scope exit aborts: borrowed executable stacks cannot safely escape their environment.

#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(panic = "abort")]
const _: () = {
    compile_error!("vthread-stack requires panic=unwind");
};

mod fiber;
mod lease;
#[doc(hidden)]
pub mod panic_payload;
mod pool;
mod scoped;

pub use fiber::{Fiber, FiberState, ParkRequest, ParkToken, SuspendError, Suspension, suspend};
pub use lease::FiberLease;
pub use pool::{StackPool, StackPoolSnapshot};
pub use scoped::{FiberScope, fiber_scope};

#[cfg(test)]
#[path = "lib_test.rs"]
mod lib_test;
