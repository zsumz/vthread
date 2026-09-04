//! Task identity, state, and scheduler-owned records.

use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard},
    time::Instant,
};

use crate::{
    PanicReport, completion::Completion, options::TaskOptions, task_progress::TaskProgress,
};

/// Stable identity assigned by one runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId(u64);

impl TaskId {
    pub(crate) fn new(value: u64) -> Self {
        assert_ne!(value, 0, "task identity zero is reserved");
        Self(value)
    }

    pub(crate) fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Stable carrier index within one runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CarrierId(pub(crate) usize);

impl CarrierId {
    /// Returns the zero-based carrier index.
    pub fn index(self) -> usize {
        self.0
    }
}

/// Why a carrier reclaimed a task without normal completion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TaskFailure {
    /// The borrowed scope exited and revoked its remaining child stacks.
    ScopeClosed,
    /// The explicit supervisor was shut down or dropped.
    SupervisorStopped,
    /// No live child could progress within the configured stall grace period.
    ScopeStalled,
    /// Runtime shutdown was requested.
    RuntimeStopped,
    /// An unexpected scheduler error stopped the owner carrier.
    CarrierFailed,
    /// The owner carrier could not allocate a task stack.
    StackAllocation,
}

/// A reason a task voluntarily returned to its carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SuspensionReason {
    /// Explicit cooperative yield.
    YieldNow,
    /// Waiting for a child stack to be reclaimed.
    Join(TaskId),
    /// Draining a borrowed local scope.
    ScopeDrain,
    /// A modeled parking generation.
    Park,
    /// Waiting for exclusive access to a virtual mutex.
    Mutex,
    /// Waiting for a condition-variable notification.
    Condvar,
    /// Waiting for a semaphore permit.
    Semaphore,
    /// Waiting for a notification permit.
    Notify,
    /// Waiting for capacity in a bounded channel.
    ChannelSend,
    /// Waiting for a value from a bounded channel.
    ChannelRecv,
    /// Waiting for socket read readiness.
    IoRead,
    /// Waiting for socket write readiness.
    IoWrite,
    /// Waiting for an incoming connection.
    IoAccept,
    /// Waiting for a nonblocking connection attempt.
    IoConnect,
    /// Waiting for explicitly delegated native work.
    Blocking,
    /// Waiting for a delegated hostname lookup.
    Dns,
    /// Waiting for delegated filesystem work.
    FileIo,
}

/// The winner that made one parked task runnable again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
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
#[non_exhaustive]
pub enum TaskStatus {
    /// An unstarted Send packet is waiting for its carrier.
    Queued,
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
    /// Reclaimed after a runtime or carrier failure.
    Aborted,
}

impl TaskStatus {
    pub(crate) fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Panicked | Self::Aborted)
    }
}

/// Immutable operator-facing task diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskSnapshot {
    /// Task identity.
    pub(crate) id: TaskId,
    /// User-supplied task name.
    pub(crate) name: String,
    /// Placement owner; the stack is created and always resumed on this carrier.
    pub(crate) carrier: CarrierId,
    /// Current park deadline, if any.
    pub(crate) deadline: Option<Instant>,
    /// Earliest inherited scope deadline.
    pub(crate) inherited_deadline: Option<Instant>,
    /// Owning root scope identity.
    pub(crate) scope: u64,
    /// Same-runtime spawning task, for transferable or borrowed children.
    pub(crate) parent: Option<TaskId>,
    /// Whether cancellation was requested by this task or an ancestor.
    pub(crate) cancellation_requested: bool,
    /// Reclamation failure, if the task was aborted.
    pub(crate) failure: Option<TaskFailure>,
    /// Current state.
    pub(crate) status: TaskStatus,
    /// Published stack mounts. While runnable this trails by at most 64 mounts;
    /// it is exact whenever the task parks or terminates.
    pub(crate) mounts: u64,
    /// Published cooperative yields. While runnable this trails by at most 64 yields;
    /// it is exact whenever the task parks or terminates.
    pub(crate) yields: u64,
    /// Number of modeled park operations.
    pub(crate) parks: u64,
    /// Most recent typed suspension boundary, if any.
    pub(crate) last_suspension: Option<SuspensionReason>,
    /// Most recent selected wake reason, if any.
    pub(crate) last_wake: Option<WakeReason>,
    /// Whether a join observed the outcome.
    pub(crate) outcome_observed: bool,
}

pub(crate) type SharedTaskRecord = Arc<TaskCell>;

/// Cache-isolated progress, completion, and mutable metadata in one shared allocation.
pub(crate) struct TaskCell {
    progress: TaskProgress,
    completion: Completion,
    record: Mutex<TaskRecord>,
}

impl TaskCell {
    pub(crate) fn new(record: TaskRecord, wait_capacity: usize) -> Self {
        Self {
            progress: TaskProgress::new(),
            completion: Completion::new(wait_capacity),
            record: Mutex::new(record),
        }
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, TaskRecord> {
        crate::signal::lock(&self.record)
    }

    pub(crate) fn progress(&self) -> &TaskProgress {
        &self.progress
    }

    pub(crate) fn completion(&self) -> &Completion {
        &self.completion
    }

    pub(crate) fn recycle(&mut self) -> crate::CancellationToken {
        self.progress.reset();
        self.completion.reset();
        let record = self
            .record
            .get_mut()
            .unwrap_or_else(|error| error.into_inner());
        assert!(
            record.status.is_terminal(),
            "recycled task must be terminal"
        );
        record.name.clear();
        record.panic = None;
        record
            .options
            .take()
            .expect("live task options")
            .cancellation
    }

    pub(crate) fn reuse(&mut self, record: TaskRecord) {
        let slot = self
            .record
            .get_mut()
            .unwrap_or_else(|error| error.into_inner());
        assert!(slot.options.is_none(), "task cell reused before recycling");
        *slot = record;
    }

    pub(crate) fn subscribe_completion(
        self: &Arc<Self>,
        wait: &crate::wait::WaitCell,
    ) -> crate::Result<crate::completion::CompletionWait> {
        self.completion.subscribe(Arc::clone(self), wait)
    }

    pub(crate) fn snapshot(&self, mounted: &[Option<TaskId>]) -> TaskSnapshot {
        let record = self.lock();
        let running = mounted.get(record.carrier.0).copied().flatten() == Some(record.id);
        TaskSnapshot {
            id: record.id,
            name: record.name.to_string(),
            carrier: record.carrier,
            deadline: record.deadline,
            inherited_deadline: record.options().deadline,
            scope: record.scope,
            parent: record.parent,
            cancellation_requested: record.options().cancellation.is_cancelled(),
            failure: record.failure,
            status: self.progress.status(record.status, running),
            mounts: self.progress.mounts(),
            yields: self.progress.yields(),
            parks: self.progress.parks(record.parks),
            last_suspension: self.progress.last_suspension(record.last_suspension),
            last_wake: self.progress.last_wake(record.last_wake),
            outcome_observed: record.outcome_observed,
        }
    }
}

pub(crate) struct TaskRecord {
    pub(crate) id: TaskId,
    pub(crate) scope: u64,
    pub(crate) parent: Option<TaskId>,
    pub(crate) options: Option<TaskOptions>,
    pub(crate) name: String,
    pub(crate) carrier: CarrierId,
    pub(crate) deadline: Option<Instant>,
    pub(crate) failure: Option<TaskFailure>,
    pub(crate) status: TaskStatus,
    pub(crate) parks: u64,
    pub(crate) last_suspension: Option<SuspensionReason>,
    pub(crate) last_wake: Option<WakeReason>,
    pub(crate) outcome_observed: bool,
    pub(crate) panic: Option<PanicReport>,
}

impl TaskRecord {
    pub(crate) fn options(&self) -> &TaskOptions {
        self.options.as_ref().expect("live task options")
    }
}

#[cfg(test)]
#[path = "task_test.rs"]
mod task_test;
