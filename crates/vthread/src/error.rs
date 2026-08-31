//! Public runtime errors and panic reports.

use std::{any::Any, error::Error as StdError, fmt, io};

use crate::{TaskFailure, TaskId};

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

impl fmt::Display for PanicReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message())
    }
}

/// A runtime, scheduling, parking, or task failure.
#[derive(Debug)]
pub enum Error {
    /// A socket, filesystem, resolver, or readiness backend operation failed.
    Io(io::Error),
    /// The readiness driver stopped after a backend failure.
    ReadinessFailed,
    /// All configured blocking-job slots are occupied.
    BlockingCapacity,
    /// A native worker failed and its pool no longer accepts work.
    BlockingFailed,
    /// The process already owns the maximum number of unfinished runtime lifecycles.
    LifecycleCapacity {
        /// Maximum runtimes, including coordinators still being joined.
        limit: usize,
    },
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
    /// A primitive reached its explicitly configured waiter limit.
    WaitQueueFull {
        /// Maximum outstanding waits, including selected but unconsumed wakes.
        limit: usize,
    },
    /// The current task or an ancestor scope requested cancellation.
    Cancelled,
    /// The earliest inherited deadline expired at a runtime boundary.
    DeadlineExceeded,
    /// The per-task context reached its configured entry limit.
    TaskLocalCapacity,
    /// A builder value violates the runtime contract.
    InvalidConfiguration {
        /// Name of the invalid field.
        field: &'static str,
        /// Human-readable constraint.
        message: &'static str,
    },
    /// The runtime has reached its configured live-task limit.
    AtCapacity {
        /// Configured admission limit.
        limit: usize,
    },
    /// A scope was entered while another scope was active on the runtime.
    NestedScope,
    /// The runtime no longer accepts work.
    RuntimeStopped,
    /// All runtime threads were joined, but one or more owned components failed.
    ShutdownFailed(Box<crate::ShutdownReport>),
    /// A blocking runtime operation was called from a virtual thread.
    InsideVThread,
    /// A native worker tried to wait for shutdown of its owning runtime.
    InsideBlockingWorker,
    /// No healthy carrier had room for another unstarted packet.
    CarrierQueueFull,
    /// An operating-system carrier thread could not be created.
    CarrierStart(io::Error),
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
    /// An internal state invariant was violated.
    Invariant(&'static str),
}

impl Error {
    pub(crate) fn invalid_configuration(field: &'static str, message: &'static str) -> Self {
        Self::InvalidConfiguration { field, message }
    }

    pub(crate) fn task_panicked(task: TaskId, name: String, panic: PanicReport) -> Self {
        Self::TaskPanicked { task, name, panic }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O: {error}"),
            Self::ReadinessFailed => formatter.write_str("readiness driver failed"),
            Self::BlockingCapacity => formatter.write_str("blocking job capacity reached"),
            Self::BlockingFailed => formatter.write_str("native blocking worker pool failed"),
            Self::LifecycleCapacity { limit } => {
                write!(formatter, "runtime lifecycle capacity {limit} reached")
            }
            Self::LimitExceeded { resource, limit } => {
                write!(formatter, "{resource} limit {limit} exceeded")
            }
            Self::BlockingPanicked(panic) => {
                write!(formatter, "blocking operation panicked: {panic}")
            }
            Self::Closed => formatter.write_str("synchronization primitive is closed"),
            Self::WouldBlock => formatter.write_str("operation would block"),
            Self::WaitQueueFull { limit } => write!(formatter, "waiter capacity {limit} reached"),
            Self::Cancelled => formatter.write_str("scope cancellation requested"),
            Self::DeadlineExceeded => formatter.write_str("inherited deadline exceeded"),
            Self::TaskLocalCapacity => formatter.write_str("task-local entry capacity reached"),
            Self::InvalidConfiguration { field, message } => {
                write!(formatter, "invalid {field}: {message}")
            }
            Self::AtCapacity { limit } => {
                write!(formatter, "virtual-thread capacity {limit} reached")
            }
            Self::NestedScope => formatter.write_str("nested runtime scopes are not supported yet"),
            Self::RuntimeStopped => formatter.write_str("runtime has stopped accepting work"),
            Self::ShutdownFailed(_) => {
                formatter.write_str("runtime shutdown retained component failures")
            }
            Self::InsideVThread => formatter.write_str(
                "this operation blocks an OS caller and cannot run inside a virtual thread",
            ),
            Self::InsideBlockingWorker => formatter
                .write_str("a native worker cannot wait for shutdown of its owning runtime"),
            Self::CarrierQueueFull => {
                formatter.write_str("all healthy carrier ingress queues are full")
            }
            Self::CarrierStart(error) => write!(formatter, "start carrier: {error}"),
            Self::TaskAborted { task, reason } => {
                write!(formatter, "task {task} aborted: {reason:?}")
            }
            Self::OutsideVThread => formatter.write_str("no virtual thread is mounted"),
            Self::ParkerBusy => {
                formatter.write_str("parker already owns an active wait generation")
            }
            Self::DeadlineOverflow => formatter.write_str("monotonic deadline overflow"),
            Self::StackAllocation(error) => {
                write!(formatter, "allocate virtual-thread stack: {error}")
            }
            Self::TaskPanicked { task, name, panic } => {
                write!(formatter, "task {task} ({name}) panicked: {panic}")
            }
            Self::RuntimeStalled { active } => {
                write!(formatter, "runtime stalled with {active} live tasks")
            }
            Self::Invariant(message) => write!(formatter, "runtime invariant violated: {message}"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(error) | Self::StackAllocation(error) | Self::CarrierStart(error) => {
                Some(error)
            }
            _ => None,
        }
    }
}

impl From<vthread_stack::SuspendError> for Error {
    fn from(_: vthread_stack::SuspendError) -> Self {
        Self::OutsideVThread
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(test)]
#[path = "error_test.rs"]
mod error_test;
