//! Runtime and stack-pool diagnostics.

use crate::{CarrierId, TaskSnapshot};

/// Most recent automatic scope recovery, retained after its task records are reclaimed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StallSnapshot {
    /// Root scope selected for recovery.
    pub scope: u64,
    /// Monotonic detection time.
    pub detected_at: std::time::Instant,
    /// Observed quiescent interval before recovery.
    pub quiescent_for: std::time::Duration,
    /// Live tasks before abort, bounded by the configured task admission limit.
    pub tasks: Vec<TaskSnapshot>,
}

/// Cumulative scheduler counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuntimeStats {
    /// Tasks accepted by the runtime.
    pub spawned: u64,
    /// Tasks that returned normally.
    pub completed: u64,
    /// Tasks that panicked.
    pub panicked: u64,
    /// Total stack mounts.
    pub mounts: u64,
    /// Total cooperative yields.
    pub yields: u64,
    /// Total modeled park operations.
    pub parks: u64,
    /// Parked generations made runnable again.
    pub wakes: u64,
    /// Wake selections caused by monotonic deadlines.
    pub timeouts: u64,
    /// Wake selections caused by explicit cancellation.
    pub cancelled: u64,
    /// Wake selections caused by permanent close.
    pub closed: u64,
    /// Carrier sleeps while waiting for the next timer.
    pub timer_sleeps: u64,
    /// Wake notices ignored after their generation was no longer parked.
    pub stale_wakes: u64,
    /// Tasks discarded while recovering a stalled scope.
    pub aborted: u64,
    /// Spawn attempts rejected at capacity.
    pub rejected: u64,
}

/// Bounded stack-cache counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StackSnapshot {
    /// Stacks currently retained.
    pub cached: usize,
    /// Fresh stack mappings created.
    pub allocated: u64,
    /// Cached stacks reused.
    pub reused: u64,
    /// Completed stacks retained.
    pub retained: u64,
    /// Completed stacks discarded at the cache limit.
    pub discarded: u64,
}

impl From<vthread_stack::StackPoolSnapshot> for StackSnapshot {
    fn from(snapshot: vthread_stack::StackPoolSnapshot) -> Self {
        Self {
            cached: snapshot.cached,
            allocated: snapshot.allocated,
            reused: snapshot.reused,
            retained: snapshot.retained,
            discarded: snapshot.discarded,
        }
    }
}

/// Health of one persistent carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarrierStatus {
    /// Waiting for start packets, wakes, or timers.
    Idle,
    /// Executing a task or scheduler transition.
    Running,
    /// Shut down and reclaimed all owned stacks.
    Stopped,
    /// Reclaimed its work after an unexpected scheduler failure.
    Failed,
}

/// Owner-local scheduler counters published by a carrier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarrierSnapshot {
    /// Stable runtime-local identity.
    pub id: CarrierId,
    /// Current carrier health.
    pub status: CarrierStatus,
    /// Tasks with retained stacks or unstarted admission.
    pub active: usize,
    /// Local runnable stacks.
    pub runnable: usize,
    /// Local parked stacks.
    pub parked: usize,
    /// Active monotonic timers.
    pub timers: usize,
    /// Unstarted packets waiting in the bounded inbox.
    pub pending_starts: usize,
    /// Selected wake notices waiting in reserved slots.
    pub pending_wakes: usize,
    /// Cumulative carrier counters.
    pub stats: RuntimeStats,
    /// Carrier-local stack cache.
    pub stacks: StackSnapshot,
}

impl CarrierSnapshot {
    pub(crate) fn new(id: CarrierId) -> Self {
        Self {
            id,
            status: CarrierStatus::Idle,
            active: 0,
            runnable: 0,
            parked: 0,
            timers: 0,
            pending_starts: 0,
            pending_wakes: 0,
            stats: RuntimeStats::default(),
            stacks: StackSnapshot::default(),
        }
    }
}

/// Point-in-time runtime state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeSnapshot {
    /// Most recent stalled scope; only one bounded report is retained per runtime.
    pub last_stall: Option<StallSnapshot>,
    /// Readiness registration and delegated native-work bounds and activity.
    pub services: crate::ServiceSnapshot,
    /// Per-carrier health and ownership counters.
    pub carriers: Vec<CarrierSnapshot>,
    /// Number of live tasks.
    pub active: usize,
    /// Number of tasks waiting in the run queue.
    pub runnable: usize,
    /// Number of tasks parked on wait generations.
    pub parked: usize,
    /// Number of active monotonic timers.
    pub timers: usize,
    /// Cumulative scheduler counters.
    pub stats: RuntimeStats,
    /// Stack-cache counters.
    pub stacks: StackSnapshot,
    /// Task records retained by the active scope.
    pub tasks: Vec<TaskSnapshot>,
}

impl RuntimeStats {
    pub(crate) fn add(&mut self, other: Self) {
        self.spawned += other.spawned;
        self.completed += other.completed;
        self.panicked += other.panicked;
        self.mounts += other.mounts;
        self.yields += other.yields;
        self.parks += other.parks;
        self.wakes += other.wakes;
        self.timeouts += other.timeouts;
        self.cancelled += other.cancelled;
        self.closed += other.closed;
        self.timer_sleeps += other.timer_sleeps;
        self.stale_wakes += other.stale_wakes;
        self.aborted += other.aborted;
        self.rejected += other.rejected;
    }
}

impl StackSnapshot {
    pub(crate) fn add(&mut self, other: Self) {
        self.cached += other.cached;
        self.allocated += other.allocated;
        self.reused += other.reused;
        self.retained += other.retained;
        self.discarded += other.discarded;
    }
}

#[cfg(test)]
#[path = "diagnostics_test.rs"]
mod diagnostics_test;
