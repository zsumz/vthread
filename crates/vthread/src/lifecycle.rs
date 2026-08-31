//! Explicit runtime and supervisor shutdown ownership and reports.
pub use crate::lifecycle_owner::{LIFECYCLE_CAPACITY, LifecycleHealth, lifecycle_health};
pub use crate::runtime::ShutdownOutcome;
pub use crate::supervisor::{ShutdownReport, Supervisor, SupervisorShutdownOutcome};
#[cfg(test)]
#[path = "lifecycle_test.rs"]
mod lifecycle_test;
