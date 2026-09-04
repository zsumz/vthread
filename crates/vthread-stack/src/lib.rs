//! Private, unsupported stack backend for `vthread`; not an independent public API.
//!
//! Direct downstream use has no compatibility contract. Cleanup mount callbacks are
//! runtime integration hooks and must succeed. A persistent mount failure during lexical
//! scope exit aborts: borrowed executable stacks cannot safely escape their environment.
//!
//! Two context engines exist while the native engine is qualified. The default keeps
//! corosensei switching contexts on vthread-owned mappings; building with
//! `--cfg vthread_stack_engine="native"` selects the native engine instead.

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

#[cfg(all(
    vthread_stack_engine = "native",
    not(all(target_os = "macos", target_arch = "aarch64"))
))]
const _: () = {
    compile_error!("the native vthread-stack engine currently supports macOS ARM64 only");
};

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

// Both engines always compile so every target keeps checking both; only one is selected.
#[cfg_attr(not(vthread_stack_engine = "native"), allow(dead_code))]
mod arch;
#[cfg_attr(not(vthread_stack_engine = "native"), allow(dead_code))]
mod context;
#[cfg_attr(vthread_stack_engine = "native", allow(dead_code))]
mod engine_corosensei;
#[cfg_attr(not(vthread_stack_engine = "native"), allow(dead_code))]
mod entry;
#[cfg_attr(not(vthread_stack_engine = "native"), allow(dead_code))]
mod native;

#[cfg(not(vthread_stack_engine = "native"))]
use engine_corosensei as engine;
#[cfg(vthread_stack_engine = "native")]
use native as engine;

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
