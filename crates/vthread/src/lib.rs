//! Carrier-affine virtual threads with explicit suspension boundaries.
//!
//! ```
//! fn main() -> vthread::Result<()> {
//!     vthread::run(|scope| {
//!         let mut answer = scope.spawn("answer", || 42)?;
//!         println!("{}", answer.join()?);
//!         Ok(())
//!     })
//! }
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[cfg(panic = "abort")]
const _: () = {
    compile_error!("vthread requires panic=unwind");
};

#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
)))]
const _: () = {
    compile_error!("vthread 0.0.2-rc.1 supports Linux x86_64 and macOS ARM64");
};

pub mod blocking;
mod cancellation;
mod carrier;
pub mod channel;
mod completion;
mod config;
mod context;
mod control;
mod diagnostic_text;
pub mod diagnostics;
mod diagnostics_accessors;
mod dump;
pub mod error;
pub mod fs;
mod identity;
mod inbox;
mod join;
mod join_slot;
mod join_wait;
mod kernel;
pub mod lifecycle;
mod lifecycle_owner;
mod lifecycle_resources;
mod local_carrier;
mod local_join;
mod local_scope;
mod metrics_accessors;
pub mod net;
mod options;
pub mod parking;
mod readiness;
mod runner;
mod runtime;
mod scope;
mod scope_failure;
mod scope_failure_report;
mod services;
mod signal;
mod spawner;
mod stall_policy;
mod supervisor;
pub mod sync;
mod task;
mod task_accessors;
mod task_body;
mod task_context;
mod task_fiber;
mod thread_failure;
mod time;
mod timer;
mod wait;
mod wait_hub;
mod worker_context;

pub use cancellation::CancellationToken;
pub use config::{RuntimeBuilder, RuntimeConfig};
pub use context::{cancellation_token, checkpoint, deadline};
pub(crate) use diagnostics::{
    CarrierSnapshot, CarrierStatus, RuntimeSnapshot, RuntimeStats, ShutdownPhase, StackSnapshot,
    StallSnapshot,
};
pub(crate) use error::PanicReport;
pub use error::{Error, Result};
pub use join::JoinHandle;
pub use local_join::LocalJoinHandle;
pub use local_scope::{
    LocalScope, local_scope, local_scope_with_deadline, try_local_scope,
    try_local_scope_with_deadline,
};
pub use options::{ScopeOptions, SpawnOptions};
pub(crate) use parking::{ParkOutcome, Parker, park_pair};
#[cfg(test)]
use parking::{UnparkResult, Unparker};
pub use runtime::Runtime;
#[cfg(test)]
use runtime::ShutdownOutcome;
pub use scope::Scope;
pub(crate) use scope_failure::ScopeFailure;
pub(crate) use services::ServiceSnapshot;
pub use spawner::Spawner;
pub use stall_policy::StallPolicy;
pub(crate) use supervisor::ShutdownReport;
#[cfg(test)]
use supervisor::SupervisorShutdownOutcome;
pub(crate) use task::{
    CarrierId, SuspensionReason, TaskFailure, TaskId, TaskSnapshot, TaskStatus, WakeReason,
};
pub use task_context::TaskLocal;
pub(crate) use thread_failure::{FailurePhase, ThreadComponent, ThreadFailure, ThreadFailures};
pub use time::{sleep, sleep_until};

/// Cooperatively yields the current virtual thread to the carrier scheduler.
pub fn yield_now() -> Result<()> {
    checkpoint()?;
    vthread_stack::suspend(vthread_stack::Suspension::YieldNow).map_err(Error::from)?;
    checkpoint()
}

/// Runs one structured scope and explicitly shuts down its default runtime.
/// Returns a shutdown error even when the scope succeeds. If both fail,
/// [`error::RunFailure`] preserves both causes in [`Error::RunFailed`].
pub fn run<R>(body: impl FnOnce(&Scope<'_>) -> Result<R>) -> Result<R> {
    if context::current().is_some() {
        return Err(Error::InsideVThread);
    }
    runner::run(Runtime::new()?, body)
}

/// Runs an application-error scope and explicitly shuts down its default runtime.
///
/// The body error stays caller-owned and needs no `Send`, formatting, or `'static`
/// bound. [`error::ApplicationRunFailure`] independently preserves body, structured
/// scope/runtime, and shutdown failures. Failure to construct a runtime is returned
/// in its `scope` field without running the body. Partial initialization is explicitly
/// shut down; an independent cleanup failure appears in its `shutdown` field.
/// Body panics resume unwinding after child reclamation; runtime Drop then attempts
/// shutdown during unwind, so shutdown failures cannot be returned with that panic.
///
/// ```
/// let input = String::from("invalid input");
/// let failure = vthread::try_run(|_| Err::<(), _>(&input)).unwrap_err();
/// assert_eq!(failure.body(), Some(&&input));
/// assert!(failure.scope().is_none());
/// assert!(failure.shutdown().is_none());
/// ```
pub fn try_run<R, E>(
    body: impl FnOnce(&Scope<'_>) -> std::result::Result<R, E>,
) -> std::result::Result<R, error::ApplicationRunFailure<E>> {
    if context::current().is_some() {
        return Err(error::ApplicationRunFailure::runtime(Error::InsideVThread));
    }
    let runtime = Runtime::new().map_err(error::ApplicationRunFailure::runtime)?;
    runner::try_run(runtime, body)
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod lib_test;

#[cfg(test)]
#[path = "support_test.rs"]
mod support_test;

#[cfg(test)]
#[path = "multicarrier_test.rs"]
mod multicarrier_test;

#[cfg(test)]
#[path = "shutdown_test.rs"]
mod shutdown_test;

#[cfg(test)]
#[path = "child_control_test.rs"]
mod child_control_test;
