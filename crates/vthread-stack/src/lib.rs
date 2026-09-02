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

#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
)))]
const _: () = {
    compile_error!("vthread-stack 0.0.2-rc.1 supports Linux x86_64 and macOS ARM64");
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
