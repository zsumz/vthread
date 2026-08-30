//! Public runtime errors and panic reports.

use std::{
    any::Any,
    error::Error as StdError,
    fmt,
    io,
};

use crate::TaskId;

/// The result type returned by `vthread` operations.
pub type Result<T> = std::result::Result<T, Error>;

/// A stable, transportable description of a task panic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanicReport {
    message: String,
}

impl PanicReport {
    pub(crate) fn capture(payload: Box<dyn Any + Send>) -> Self {
        let message = payload
            .downcast_ref::<&'static str>()
            .map(|value| (*value).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic payload".to_owned());
        Self { message }
    }

    /// Returns the panic message or a fallback for non-string payloads.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for PanicReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

/// A runtime, scheduling, or task failure.
#[derive(Debug)]
pub enum Error {
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
    /// Suspension was attempted without a mounted virtual thread.
    OutsideVThread,
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
    /// Live tasks existed but no runnable task could make progress.
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
            Self::InvalidConfiguration { field, message } => {
                write!(formatter, "invalid {field}: {message}")
            }
            Self::AtCapacity { limit } => {
                write!(formatter, "virtual-thread capacity {limit} reached")
            }
            Self::NestedScope => formatter.write_str("nested runtime scopes are not supported yet"),
            Self::OutsideVThread => formatter.write_str("no virtual thread is mounted"),
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
            Self::StackAllocation(error) => Some(error),
            _ => None,
        }
    }
}

impl From<vthread_stack::SuspendError> for Error {
    fn from(_: vthread_stack::SuspendError) -> Self {
        Self::OutsideVThread
    }
}

#[cfg(test)]
#[path = "error_test.rs"]
mod error_test;
