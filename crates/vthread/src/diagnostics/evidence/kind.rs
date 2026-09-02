//! Typed payloads for authoritative runtime evidence transitions.

use super::{StackId, WaitKey, kind_types::*};
use crate::diagnostics::{CarrierId, ScopeId, ShutdownPhase, SuspensionReason, TaskId};

/// One authoritative runtime state transition.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
#[non_exhaustive]
pub enum RuntimeEventKind {
    /// An owned root or supervisor scope became active.
    /// Borrowed local scopes do not receive separate scope identities or events.
    ScopeOpened {
        /// New runtime-local scope identity.
        scope: ScopeId,
        /// Always `None` in this release because every owned scope is top-level.
        parent: Option<ScopeId>,
        /// Whether this is a long-lived supervisor.
        supervised: bool,
    },
    /// An owned root or supervisor drained and released its retained records.
    ScopeClosed {
        /// Drained runtime-local scope identity.
        scope: ScopeId,
    },
    /// A task became irrevocably owned by the runtime.
    TaskAccepted {
        /// New monotonic task identity.
        task: TaskId,
        /// Owning root or supervisor.
        scope: ScopeId,
        /// Same-runtime spawning task, when present.
        parent: Option<TaskId>,
        /// Immutable owner carrier.
        carrier: CarrierId,
    },
    /// A reusable stack mapping became owned by a task.
    StackCheckedOut {
        /// Task receiving the mapping.
        task: TaskId,
        /// Reusable mapping identity.
        stack: StackId,
    },
    /// A task began one carrier-affine execution turn.
    Mounted {
        /// Mounted task.
        task: TaskId,
        /// Carrier performing the mount.
        carrier: CarrierId,
    },
    /// A task voluntarily yielded its execution turn.
    Yielded {
        /// Yielding task.
        task: TaskId,
        /// Carrier retaining the task.
        carrier: CarrierId,
    },
    /// A wait generation was registered with its carrier.
    WaitPublished {
        /// Waiting task.
        task: TaskId,
        /// Newly active generation.
        wait: WaitKey,
        /// Whether the park requested a timer.
        has_deadline: bool,
    },
    /// The carrier transferred a task into its parked table.
    Parked {
        /// Parked task.
        task: TaskId,
        /// Exact active generation.
        wait: WaitKey,
        /// Owner carrier.
        carrier: CarrierId,
        /// Typed suspension boundary.
        reason: SuspensionReason,
    },
    /// A producer attempted to select one exact wait generation.
    WakeOffered {
        /// Task captured by the generation registration.
        task: TaskId,
        /// Exact offered generation.
        wait: WaitKey,
        /// Offered wake cause.
        cause: EvidenceWakeCause,
        /// Context that offered this wake.
        origin: WakeOrigin,
    },
    /// The offered cause became the generation's sole winner.
    WakeSelected {
        /// Selected task.
        task: TaskId,
        /// Selected generation.
        wait: WaitKey,
        /// Winning wake cause.
        cause: EvidenceWakeCause,
        /// Context that offered the winning wake.
        origin: WakeOrigin,
    },
    /// The offered cause could not select the requested generation.
    WakeRejected {
        /// Task captured by the offered registration.
        task: TaskId,
        /// Rejected generation.
        wait: WaitKey,
        /// Rejected wake cause.
        cause: EvidenceWakeCause,
        /// Context that offered the rejected wake.
        origin: WakeOrigin,
        /// Exact rejection reason.
        reason: WakeRejection,
    },
    /// Task execution returned from a modeled park operation.
    Resumed {
        /// Resumed task.
        task: TaskId,
        /// Generation being completed.
        wait: WaitKey,
        /// Previously selected winner.
        cause: EvidenceWakeCause,
    },
    /// A wait generation entered a carrier's timer queue.
    TimerRegistered {
        /// Timed generation.
        wait: WaitKey,
        /// Owner carrier.
        carrier: CarrierId,
    },
    /// A wait generation left a carrier's timer queue.
    TimerRetired {
        /// Retired timed generation.
        wait: WaitKey,
        /// Owner carrier.
        carrier: CarrierId,
        /// Retirement cause.
        reason: TimerRetirement,
    },
    /// Task ownership of a stack mapping ended.
    StackReleased {
        /// Previous task owner.
        task: TaskId,
        /// Released mapping.
        stack: StackId,
        /// Cache or unmap disposition.
        disposition: StackDisposition,
    },
    /// A task reached its single terminal runtime state.
    TaskTerminated {
        /// Terminal task.
        task: TaskId,
        /// Runtime-observed terminal state.
        outcome: TaskOutcome,
    },
    /// A bounded carrier queue changed depth.
    QueueDepth {
        /// Queue owner carrier.
        carrier: CarrierId,
        /// Queue category.
        queue: QueueKind,
        /// Depth after the transition.
        depth: usize,
        /// Immutable configured limit.
        capacity: usize,
    },
    /// Bounded runtime resource admission rejected work.
    AdmissionRejected {
        /// Resource that rejected admission.
        resource: crate::error::CapacityResource,
        /// Configured resource limit.
        limit: usize,
    },
    /// Runtime shutdown advanced to a nondecreasing phase.
    ShutdownAdvanced {
        /// Newly visible shutdown phase.
        phase: ShutdownPhase,
    },
}

#[cfg(test)]
#[path = "kind_test.rs"]
mod kind_test;
