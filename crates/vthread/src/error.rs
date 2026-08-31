//! Public runtime errors and panic reports.

#[path = "error_display.rs"]
mod error_display;

use std::{any::Any, io};

pub use crate::scope_failure::ScopeFailure;
use crate::{TaskFailure, TaskId};
#[path = "error_context.rs"]
mod error_context;
pub use error_context::{CapacityResource, FaultComponent, IoFailure, RuntimeFault};

/// The result type returned by `vthread` operations.
pub type Result<T> = std::result::Result<T, Error>;

/// A stable, transportable description of a task panic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanicReport {
    captured: vthread_stack::panic_payload::CapturedPanic,
}

impl PanicReport {
    pub(crate) fn capture(payload: Box<dyn Any + Send>) -> Self {
        Self {
            captured: vthread_stack::panic_payload::capture(payload),
        }
    }

    pub(crate) fn from_captured(captured: vthread_stack::panic_payload::CapturedPanic) -> Self {
        Self { captured }
    }

    /// Returns the panic message or a fallback for non-string payloads.
    pub fn message(&self) -> &str {
        &self.captured.message
    }

    /// Whether the original message exceeded the 1024-byte UTF-8 text limit.
    pub fn truncated(&self) -> bool {
        self.captured.truncated
    }

    /// Whether disposing of the panic payload itself panicked and failed its owner.
    pub fn cleanup_panicked(&self) -> bool {
        self.captured.cleanup_panicked
    }
}

/// A runtime, scheduling, parking, or task failure.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// A socket, filesystem, resolver, or readiness backend operation failed.
    Io(IoFailure),
    /// The readiness driver stopped after a backend failure.
    ReadinessFailed,
    /// A native worker failed and its pool no longer accepts work.
    BlockingFailed,
    /// A bounded I/O result exceeded its requested limit.
    LimitExceeded {
        /// Which result was bounded.
        resource: &'static str,
        /// Configured maximum size or count.
        limit: usize,
    },
    /// A delegated blocking operation panicked.
    BlockingPanicked(PanicReport),
    /// A synchronization primitive or channel no longer accepts this operation.
    Closed,
    /// A nonblocking synchronization operation cannot complete immediately.
    WouldBlock,
    /// This handle's terminal result has already been consumed.
    ResultAlreadyTaken,
    /// The current task or an ancestor scope requested cancellation.
    Cancelled,
    /// The earliest inherited deadline expired at a runtime boundary.
    DeadlineExceeded,
    /// A task-local initializer tried to read the same key before initialization completed.
    RecursiveTaskLocal,
    /// Fallible reservation could not grow an explicitly bounded result buffer.
    AllocationFailed {
        /// Bounded resource being allocated.
        resource: &'static str,
        /// Requested total buffer capacity in bytes.
        requested: usize,
    },
    /// A builder value violates the runtime contract.
    InvalidConfiguration {
        /// Name of the invalid field.
        field: &'static str,
        /// Human-readable constraint.
        message: &'static str,
    },
    /// A bounded resource reached its admission limit.
    Capacity {
        /// Which resource rejected the request.
        resource: CapacityResource,
        /// Configured admission limit.
        limit: usize,
    },
    /// A scope was entered while another scope was active on the runtime.
    RootScopeActive,
    /// The runtime no longer accepts work.
    RuntimeStopped,
    /// A structured scope retained body, cleanup, policy or child failures.
    ScopeFailed(std::sync::Arc<crate::ScopeFailure>),
    /// All runtime threads were joined, but one or more owned components failed.
    ShutdownFailed(Box<crate::ShutdownReport>),
    /// A blocking runtime operation was called from a virtual thread.
    InsideVThread,
    /// A native worker tried to wait for shutdown of its owning runtime.
    InsideBlockingWorker,
    /// An owned operating-system thread could not be created.
    ThreadStart {
        /// Component that could not start.
        component: crate::ThreadComponent,
        /// Original operating-system error.
        source: io::Error,
    },
    /// A task attempted to wait for itself.
    JoinSelf,
    /// A child stack was reclaimed without a normal result.
    TaskAborted {
        /// Identity of the aborted task.
        task: TaskId,
        /// Why the task was reclaimed.
        reason: TaskFailure,
    },
    /// Suspension was attempted without a mounted virtual thread.
    OutsideVThread,
    /// One parker was asked to own two active generations simultaneously.
    ParkerBusy,
    /// A relative duration could not be represented as a monotonic deadline.
    DeadlineOverflow,
    /// A guard-page-backed stack could not be allocated.
    StackAllocation(io::Error),
    /// A joined or unobserved child task panicked.
    TaskPanicked {
        /// Identity of the failed task.
        task: TaskId,
        /// Operator-visible task name.
        name: String,
        /// Captured panic description.
        panic: PanicReport,
    },
    /// Live tasks existed but no runnable task or timer could make progress.
    RuntimeStalled {
        /// Number of live tasks at the point of failure.
        active: usize,
    },
    /// An internal fault with opaque incident identity and component metadata.
    Fault(RuntimeFault),
}

impl Error {
    /// Returns structured scope details without discarding any secondary failure.
    pub fn scope_failure(&self) -> Option<&crate::ScopeFailure> {
        if let Self::ScopeFailed(failure) = self {
            Some(failure)
        } else {
            None
        }
    }

    /// Returns the representative causal error through any enclosing scope failures.
    /// Inspect scope_failure as well when secondary failures affect recovery.
    pub fn primary(&self) -> &Self {
        let mut current = self;
        while let Self::ScopeFailed(failure) = current {
            let Some(next) = failure.primary() else {
                break;
            };
            current = next;
        }
        current
    }
    pub(crate) fn fault(component: FaultComponent, detail: &'static str) -> Self {
        Self::Fault(RuntimeFault::new(component, detail))
    }
    pub(crate) fn io(
        operation: &'static str,
        context: impl std::fmt::Display,
        source: io::Error,
    ) -> Self {
        Self::Io(IoFailure::new(operation, context, source))
    }
    pub(crate) fn thread_start(component: crate::ThreadComponent, source: io::Error) -> Self {
        Self::ThreadStart { component, source }
    }
    pub(crate) fn invalid_configuration(field: &'static str, message: &'static str) -> Self {
        Self::InvalidConfiguration { field, message }
    }

    pub(crate) fn task_panicked(task: TaskId, name: String, panic: PanicReport) -> Self {
        Self::TaskPanicked { task, name, panic }
    }
}

#[cfg(test)]
#[path = "error_test.rs"]
mod error_test;
