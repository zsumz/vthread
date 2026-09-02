//! Value types carried by authoritative runtime evidence transitions.

use crate::diagnostics::CarrierId;

/// Cause offered to one exact wait generation.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
#[non_exhaustive]
pub enum EvidenceWakeCause {
    /// Readiness or an explicit unpark operation.
    Ready,
    /// The generation's monotonic deadline expired.
    TimedOut,
    /// Explicit cancellation of the parking primitive.
    Cancelled,
    /// Cancellation inherited from task or scope ownership.
    InheritedCancelled,
    /// The parking primitive or service closed permanently.
    Closed,
}

/// Execution context that offered a wake to one exact generation.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
#[non_exhaustive]
pub enum WakeOrigin {
    /// The wake was offered by this runtime carrier.
    Carrier(CarrierId),
    /// The wake was offered outside a carrier, including service and ordinary OS threads.
    External,
}

/// Why an offered wake did not select its generation.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
#[non_exhaustive]
pub enum WakeRejection {
    /// The wait object no longer exists.
    NoWait,
    /// The wait object currently owns another generation.
    RetiredGeneration,
    /// The generation already has a selected winner.
    AlreadySelected,
    /// The wait object has no active parked generation.
    NoActiveWait,
}

/// Final runtime-observed task state.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
#[non_exhaustive]
pub enum TaskOutcome {
    /// The task entry function returned.
    Completed,
    /// The task entry function unwound with a panic.
    Panicked,
    /// The runtime reclaimed the task without normal completion.
    Aborted(crate::diagnostics::TaskFailure),
}

/// Why an active timer left its carrier-local timer queue.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
#[non_exhaustive]
pub enum TimerRetirement {
    /// Its monotonic deadline expired.
    Expired,
    /// Another wake cause selected the same wait generation.
    WakeSelected,
    /// Task reclamation removed the timer.
    TaskReclaimed,
}

/// What happened to a stack after task ownership ended.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
#[non_exhaustive]
pub enum StackDisposition {
    /// The mapping entered its carrier's bounded cache.
    Cached,
    /// The mapping was unmapped instead of entering the cache.
    Discarded,
}

/// Runtime queue whose exact depth changed.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
#[non_exhaustive]
pub enum QueueKind {
    /// Transferable task packets waiting for stack creation.
    Start,
    /// Borrowed carrier-local tasks waiting to join the ready queue.
    LocalStart,
    /// Selected wake notices waiting for scheduler delivery.
    Wake,
}

#[cfg(test)]
#[path = "kind_types_test.rs"]
mod kind_types_test;
