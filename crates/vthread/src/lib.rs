//! Carrier-affine virtual threads with explicit suspension boundaries.

#![forbid(unsafe_code)]

mod config;
mod diagnostics;
mod error;
mod join;
mod kernel;
mod runtime;
mod scope;
mod task;

pub use config::{RuntimeBuilder, RuntimeConfig};
pub use diagnostics::{RuntimeSnapshot, RuntimeStats, StackSnapshot};
pub use error::{Error, PanicReport, Result};
pub use join::JoinHandle;
pub use runtime::Runtime;
pub use scope::Scope;
pub use task::{SuspensionReason, TaskId, TaskSnapshot, TaskStatus};

/// Cooperatively yields the current virtual thread to the carrier scheduler.
pub fn yield_now() -> Result<()> {
    vthread_stack::suspend(vthread_stack::Suspension::YieldNow).map_err(Error::from)
}

/// Runs one structured scope on a runtime with default configuration.
pub fn run<R>(body: impl FnOnce(&Scope<'_>) -> Result<R>) -> Result<R> {
    Runtime::new()?.scope(body)
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod lib_test;
