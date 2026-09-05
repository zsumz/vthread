//! Private, unsupported stack backend for `vthread`; not an independent public API.
//!
//! Direct downstream use has no compatibility contract. Cleanup mount callbacks are
//! runtime integration hooks and must succeed. A persistent mount failure during lexical
//! scope exit aborts: borrowed executable stacks cannot safely escape their environment.
//!
//! Layers, lowest first:
//!
//! - `stack_unix` and `stack`: guard-page-backed mappings stamped with a pool identity.
//! - `arch`: per-target context switch and first-frame fabrication, nothing more.
//! - `entry` and `context`: the entry closure and control block at the top of each
//!   stack, and the command and outcome protocol between a carrier and its fiber.
//! - `engine`: the fiber lifecycle state machine, including forced reclamation.
//! - `mount`, `fiber`, `lease`, `scoped`, and `pool`: the carrier-facing ownership
//!   model the scheduler consumes.

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

mod arch;
mod context;
mod engine;
mod entry;
mod fiber;
mod lease;
mod mount;
#[doc(hidden)]
pub mod panic_payload;
mod pool;
mod scoped;
mod stack;
mod stack_unix;
mod suspension;

pub use fiber::Fiber;
pub use lease::FiberLease;
#[doc(hidden)]
pub use mount::ContextKey;
pub use mount::suspend;
pub use pool::{StackPool, StackPoolSnapshot};
pub use scoped::{FiberScope, fiber_scope};
pub use stack::{MappedStack, STACK_ALIGNMENT};
pub use suspension::{FiberState, ParkRequest, ParkToken, Resume, SuspendError, Suspension};

#[cfg(test)]
#[path = "lib_test.rs"]
mod lib_test;
#[cfg(test)]
#[path = "terminal_test.rs"]
mod terminal_test;
#[cfg(test)]
#[path = "trace_test.rs"]
mod trace_test;
