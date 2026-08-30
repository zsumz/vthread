//! Carrier-affine virtual threads with explicit suspension boundaries.
//!
//! ```
//! use std::{rc::Rc, thread, time::Duration};
//! use vthread::{ParkOutcome, Runtime, park_pair};
//!
//! fn main() -> vthread::Result<()> {
//!     let runtime = Runtime::builder().carriers(2).build()?;
//!     runtime.scope(|scope| {
//!         let (parker, unparker) = park_pair();
//!         let waiter = scope.spawn("waiter", move || {
//!             let owner = thread::current().id();
//!             let local = Rc::new(42);
//!             let outcome = parker.park_timeout(Duration::from_secs(5))?;
//!             assert_eq!(thread::current().id(), owner);
//!             Ok::<_, vthread::Error>((*local, outcome))
//!         })?;
//!         thread::spawn(move || unparker.unpark()).join().expect("remote waker");
//!         assert_eq!(waiter.join()??, (42, ParkOutcome::Ready));
//!         Ok(())
//!     })?;
//!     runtime.shutdown().map(|_| ())
//! }
//! ```

#![forbid(unsafe_code)]

pub mod blocking;
mod cancellation;
mod carrier;
pub mod channel;
mod completion;
mod config;
mod context;
mod control;
mod diagnostics;
mod error;
pub mod fs;
mod inbox;
mod join;
mod join_wait;
mod kernel;
mod local_carrier;
mod local_join;
mod local_scope;
pub mod net;
mod options;
mod parking;
mod readiness;
mod runtime;
mod scope;
mod services;
mod signal;
mod supervisor;
pub mod sync;
mod task;
mod task_body;
mod task_context;
mod task_fiber;
mod time;
mod timer;
mod wait;
mod wait_hub;

pub use cancellation::CancellationToken;
pub use config::{RuntimeBuilder, RuntimeConfig};
pub use context::{cancellation_token, checkpoint, deadline};
pub use diagnostics::{
    CarrierSnapshot, CarrierStatus, RuntimeSnapshot, RuntimeStats, StackSnapshot,
};
pub use error::{Error, PanicReport, Result};
pub use join::JoinHandle;
pub use local_join::LocalJoinHandle;
pub use local_scope::{LocalScope, local_scope, local_scope_with_deadline};
pub use options::ScopeOptions;
pub use parking::{ParkOutcome, Parker, UnparkResult, Unparker, park_pair};
pub use runtime::Runtime;
pub use scope::Scope;
pub use services::ServiceSnapshot;
pub use supervisor::{ShutdownReport, Supervisor};
pub use task::{
    CarrierId, SuspensionReason, TaskFailure, TaskId, TaskSnapshot, TaskStatus, WakeReason,
};
pub use task_context::TaskLocal;
pub use time::{sleep, sleep_until};

/// Cooperatively yields the current virtual thread to the carrier scheduler.
pub fn yield_now() -> Result<()> {
    checkpoint()?;
    vthread_stack::suspend(vthread_stack::Suspension::YieldNow).map_err(Error::from)?;
    checkpoint()
}

/// Runs one structured scope on a runtime with default configuration.
pub fn run<R>(body: impl FnOnce(&Scope<'_>) -> Result<R>) -> Result<R> {
    Runtime::new()?.scope(body)
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
