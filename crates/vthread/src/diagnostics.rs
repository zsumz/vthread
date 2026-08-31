//! Runtime and stack-pool diagnostics.
//!
//! Diagnostic records are observations with private fields, not scheduler inputs.
//! ```compile_fail
//! let snapshot = vthread::diagnostics::RuntimeSnapshot { active: 0 };
//! ```
//! Extensible diagnostic enums require a fallback arm downstream.
//! ```compile_fail
//! use vthread::diagnostics::CarrierStatus;
//! fn exhaustive(status: CarrierStatus) { match status {
//!     CarrierStatus::Idle | CarrierStatus::Running | CarrierStatus::Stopped | CarrierStatus::Failed => ()
//! } }
//! ```

pub use crate::dump::DumpReport;
pub use crate::identity::{RuntimeId, ScopeId};
pub use crate::services::ServiceSnapshot;
pub use crate::task::{
    CarrierId, SuspensionReason, TaskFailure, TaskId, TaskSnapshot, TaskStatus, WakeReason,
};
pub use crate::thread_failure::{FailurePhase, ThreadComponent, ThreadFailure, ThreadFailures};

/// Furthest shutdown stage reached; stages never move backward between concurrent callers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ShutdownPhase {
    /// No shutdown request has been made (admission may still close on carrier failure).
    #[default]
    NotRequested,
    /// Admission is closed and stop/cancellation signals are being delivered.
    Requested,
    /// Waiting for carrier threads, including their OS thread-local cleanup.
    JoiningCarriers,
    /// Carriers are joined; waiting for the readiness driver to exit.
    JoiningReadiness,
    /// Carriers/readiness are joined; waiting for native workers and their TLS cleanup.
    JoiningNative,
    /// Every owned runtime thread, including its coordinator, has been joined.
    /// The process lifecycle admission slot is released. Root callbacks are owned by
    /// ordinary OS callers and may continue; their scope invocations await their return.
    /// Shutdown cannot forcibly terminate those callbacks.
    Complete,
    /// All owned threads have been joined, but terminal component failures were retained.
    /// The process lifecycle admission slot is released; caller-owned callbacks may remain.
    Failed,
}

/// Most recent automatic scope recovery, retained after its task records are reclaimed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StallSnapshot {
    /// Explicit policy that caused this observation; reporting alone never cancels work.
    pub(crate) policy: crate::StallPolicy,
    /// Root scope selected for recovery.
    pub(crate) scope: u64,
    /// Monotonic detection time.
    pub(crate) detected_at: std::time::Instant,
    /// Observed quiescent interval before recovery.
    pub(crate) quiescent_for: std::time::Duration,
    /// Live tasks before abort, bounded by the configured task admission limit.
    pub(crate) tasks: Vec<TaskSnapshot>,
}

/// Cumulative scheduler counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuntimeStats {
    /// Tasks accepted by the runtime.
    pub(crate) admitted: u64,
    /// Tasks that returned normally.
    pub(crate) completed: u64,
    /// Tasks that panicked.
    pub(crate) panicked: u64,
    /// Total stack mounts.
    pub(crate) mounts: u64,
    /// Total cooperative yields.
    pub(crate) yields: u64,
    /// Total modeled park operations.
    pub(crate) parks: u64,
    /// Parked generations made runnable again.
    pub(crate) wakes: u64,
    /// Wake selections caused by monotonic deadlines.
    pub(crate) timeouts: u64,
    /// Wake selections caused by explicit cancellation.
    pub(crate) cancelled: u64,
    /// Wake selections caused by permanent close.
    pub(crate) closed: u64,
    /// Carrier sleeps while waiting for the next timer.
    pub(crate) timer_sleeps: u64,
    /// Wake notices ignored after their generation was no longer parked.
    pub(crate) stale_wakes: u64,
    /// Tasks discarded while recovering a stalled scope.
    pub(crate) aborted: u64,
    /// Spawn attempts rejected at capacity.
    pub(crate) rejected: u64,
}

/// Bounded stack-cache counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StackSnapshot {
    /// Stacks currently retained.
    pub(crate) cached: usize,
    /// Fresh stack mappings created.
    pub(crate) allocated: u64,
    /// Cached stacks reused.
    pub(crate) reused: u64,
    /// Completed stacks retained.
    pub(crate) retained: u64,
    /// Completed stacks discarded at the cache limit.
    pub(crate) discarded: u64,
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
#[non_exhaustive]
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
    pub(crate) id: CarrierId,
    /// Current carrier health.
    pub(crate) status: CarrierStatus,
    /// Tasks with retained stacks or unstarted admission.
    pub(crate) active: usize,
    /// Local runnable stacks.
    pub(crate) runnable: usize,
    /// Local parked stacks.
    pub(crate) parked: usize,
    /// Active monotonic timers.
    pub(crate) timers: usize,
    /// Unstarted packets waiting in the bounded inbox.
    pub(crate) pending_starts: usize,
    /// Selected wake notices waiting in reserved slots.
    pub(crate) pending_wakes: usize,
    /// Cumulative carrier counters.
    pub(crate) stats: RuntimeStats,
    /// Carrier-local stack cache.
    pub(crate) stacks: StackSnapshot,
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

/// Weakly consistent diagnostic view assembled from independently observed components.
///
/// Admission counters and record membership are captured together, then service,
/// inbox and task details are read after releasing runtime control state. Components
/// can advance between those reads; cross-field totals need not reconcile during
/// activity. This view is not a synchronization barrier or shutdown-completion proof.
#[derive(Clone, Debug)]
pub struct RuntimeSnapshot {
    pub(crate) runtime_id: RuntimeId,
    /// Most recent failed scope, retained after task records are removed.
    pub(crate) last_scope_failure:
        Option<std::sync::Arc<crate::scope_failure_report::ScopeFailureReport>>,
    /// Bounded terminal component failures, retained through shutdown.
    pub(crate) failures: crate::ThreadFailures,
    /// Current shutdown progress, including waits beyond task and native-job completion.
    pub(crate) shutdown_phase: ShutdownPhase,
    /// Whether new root scopes and task admissions are accepted (subject to capacity).
    pub(crate) accepting: bool,
    /// Most recent stalled scope; only one bounded report is retained per runtime.
    pub(crate) last_stall: Option<StallSnapshot>,
    /// Readiness registration and delegated native-work bounds and activity.
    pub(crate) services: crate::ServiceSnapshot,
    /// Per-carrier health and ownership counters.
    pub(crate) carriers: Vec<CarrierSnapshot>,
    /// Number of live tasks.
    pub(crate) active: usize,
    /// Number of tasks waiting in the run queue.
    pub(crate) runnable: usize,
    /// Number of tasks parked on wait generations.
    pub(crate) parked: usize,
    /// Number of active monotonic timers.
    pub(crate) timers: usize,
    /// Cumulative scheduler counters.
    pub(crate) stats: RuntimeStats,
    /// Stack-cache counters.
    pub(crate) stacks: StackSnapshot,
    /// Task records retained by the active scope.
    pub(crate) tasks: Vec<TaskSnapshot>,
}

impl RuntimeStats {
    pub(crate) fn add(&mut self, other: Self) {
        self.admitted += other.admitted;
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

impl RuntimeSnapshot {
    pub(crate) fn empty(runtime_id: RuntimeId) -> Self {
        Self {
            runtime_id,
            last_scope_failure: None,
            failures: Default::default(),
            shutdown_phase: ShutdownPhase::NotRequested,
            accepting: false,
            last_stall: None,
            services: Default::default(),
            carriers: Vec::new(),
            active: 0,
            runnable: 0,
            parked: 0,
            timers: 0,
            stats: Default::default(),
            stacks: Default::default(),
            tasks: Vec::new(),
        }
    }
    /// Process-unique runtime identity. Pair task, scope and carrier IDs with this value.
    pub fn runtime_id(&self) -> RuntimeId {
        self.runtime_id
    }
}
