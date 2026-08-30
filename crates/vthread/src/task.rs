//! Task identity, state, and scheduler-owned records.

use std::{
    cell::RefCell,
    fmt,
    rc::Rc,
};

use crate::PanicReport;

/// Stable identity assigned by one runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId(u64);

impl TaskId {
    pub(crate) fn new(value: u64) -> Self {
        Self(value)
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A reason a task voluntarily returned to its carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuspensionReason {
    /// Explicit cooperative yield.
    YieldNow,
    /// A modeled parking generation.
    Park,
}

/// The winner that made one parked task runnable again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WakeReason {
    /// Readiness or an explicit unpark operation.
    Ready,
    /// A monotonic deadline expired.
    TimedOut,
    /// Explicit cancellation selected the generation.
    Cancelled,
    /// The parking primitive closed permanently.
    Closed,
}

/// Current scheduler-visible task state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskStatus {
    /// Waiting in the carrier run queue.
    Ready,
    /// Mounted on the carrier stack.
    Running,
    /// Suspended at a typed runtime boundary.
    Suspended(SuspensionReason),
    /// Returned normally.
    Completed,
    /// Unwound after a panic.
    Panicked,
}

impl TaskStatus {
    pub(crate) fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Panicked)
    }
}

/// Immutable operator-facing task diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskSnapshot {
    /// Task identity.
    pub id: TaskId,
    /// User-supplied task name.
    pub name: String,
    /// Current state.
    pub status: TaskStatus,
    /// Number of times the task stack was mounted.
    pub mounts: u64,
    /// Number of cooperative yields.
    pub yields: u64,
    /// Number of modeled park operations.
    pub parks: u64,
    /// Most recent typed suspension boundary, if any.
    pub last_suspension: Option<SuspensionReason>,
    /// Most recent selected wake reason, if any.
    pub last_wake: Option<WakeReason>,
    /// Whether a join observed the outcome.
    pub outcome_observed: bool,
}

pub(crate) type SharedTaskRecord = Rc<RefCell<TaskRecord>>;

pub(crate) struct TaskRecord {
    pub(crate) id: TaskId,
    pub(crate) scope: u64,
    pub(crate) name: Rc<str>,
    pub(crate) status: TaskStatus,
    pub(crate) mounts: u64,
    pub(crate) yields: u64,
    pub(crate) parks: u64,
    pub(crate) last_suspension: Option<SuspensionReason>,
    pub(crate) last_wake: Option<WakeReason>,
    pub(crate) outcome_observed: bool,
    pub(crate) panic: Option<PanicReport>,
}

impl TaskRecord {
    pub(crate) fn snapshot(&self) -> TaskSnapshot {
        TaskSnapshot {
            id: self.id,
            name: self.name.to_string(),
            status: self.status,
            mounts: self.mounts,
            yields: self.yields,
            parks: self.parks,
            last_suspension: self.last_suspension,
            last_wake: self.last_wake,
            outcome_observed: self.outcome_observed,
        }
    }
}

#[cfg(test)]
#[path = "task_test.rs"]
mod task_test;
