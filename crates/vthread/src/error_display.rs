//! Error rendering and standard source chaining, separate from the public taxonomy.

use super::{Error, PanicReport};
use std::{error::Error as StdError, fmt, io};

impl fmt::Display for PanicReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O: {error}"),
            Self::ReadinessFailed => formatter.write_str("readiness driver failed"),
            Self::BlockingFailed => formatter.write_str("native blocking worker pool failed"),
            Self::LimitExceeded { resource, limit } => {
                write!(formatter, "{resource} limit {limit} exceeded")
            }
            Self::BlockingPanicked(panic) => {
                write!(formatter, "blocking operation panicked: {panic}")
            }
            Self::Capacity { resource, limit } => {
                write!(formatter, "{resource:?} capacity {limit} reached")
            }
            Self::JoinSelf => formatter.write_str("task cannot join itself"),
            Self::Closed => formatter.write_str("synchronization primitive is closed"),
            Self::WouldBlock => formatter.write_str("operation would block"),
            Self::ResultAlreadyTaken => formatter.write_str("task result already taken"),
            Self::Cancelled => formatter.write_str("scope cancellation requested"),
            Self::DeadlineExceeded => formatter.write_str("inherited deadline exceeded"),
            Self::RecursiveTaskLocal => formatter.write_str("recursive task-local initialization"),
            Self::AllocationFailed {
                resource,
                requested,
            } => write!(formatter, "cannot reserve {requested} bytes for {resource}"),
            Self::InvalidConfiguration { field, message } => {
                write!(formatter, "invalid {field}: {message}")
            }
            Self::RootScopeActive => {
                formatter.write_str("this application runtime already has an active root scope")
            }
            Self::RuntimeStopped => formatter.write_str("runtime has stopped accepting work"),
            Self::ScopeClosed => formatter.write_str("scope has closed child admission"),
            Self::ScopeFailed(failure) => failure.fmt(formatter),
            Self::ShutdownFailed(_) => {
                formatter.write_str("runtime shutdown retained component failures")
            }
            Self::LifecycleFailed(_) => formatter.write_str("process lifecycle owner failed"),
            Self::RunFailed(failure) => failure.fmt(formatter),
            Self::ConstructionFailed(failure) => failure.fmt(formatter),
            Self::InsideVThread => formatter.write_str(
                "this operation blocks an OS caller and cannot run inside a virtual thread",
            ),
            Self::InsideManagedWorker => formatter.write_str(
                "a managed native worker cannot perform this blocking runtime operation",
            ),
            Self::ThreadStart { component, source } => {
                write!(formatter, "start {component:?}: {source}")
            }
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
            Self::Fault(fault) => fault.fmt(formatter),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::ScopeFailed(failure) => Some(failure.as_ref()),
            Self::Io(error) => Some(error),
            Self::RunFailed(failure) => Some(failure.as_ref()),
            Self::ConstructionFailed(failure) => Some(failure.as_ref()),
            Self::StackAllocation(error) | Self::ThreadStart { source: error, .. } => Some(error),
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
        Self::io("runtime I/O", "unspecified operation", error)
    }
}

#[cfg(test)]
#[path = "error_display_test.rs"]
mod error_display_test;
