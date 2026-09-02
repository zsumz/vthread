//! Fixed-size runtime evidence records and opaque correlation identities.

#[path = "kind.rs"]
mod kind;
#[path = "kind_types.rs"]
mod kind_types;
pub use kind::RuntimeEventKind;
pub use kind_types::{
    EvidenceWakeCause, QueueKind, StackDisposition, TaskOutcome, TimerRetirement, WakeOrigin,
    WakeRejection,
};

use crate::diagnostics::{CarrierId, RuntimeId};
/// Monotonic runtime-wide publication sequence.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
    ::core::cmp::PartialOrd,
    ::core::cmp::Ord,
    ::core::hash::Hash,
)]
pub struct EventSequence(u64);

impl EventSequence {
    pub(crate) fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the raw sequence value, beginning at zero.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// One generation of a reusable runtime wait object.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
    ::core::cmp::PartialOrd,
    ::core::cmp::Ord,
    ::core::hash::Hash,
)]
pub struct WaitKey {
    wait: u64,
    generation: u64,
}

impl WaitKey {
    pub(crate) fn from_token(token: vthread_stack::ParkToken) -> Self {
        Self {
            wait: token.wait(),
            generation: token.generation(),
        }
    }

    /// Returns the process-local wait-object identity.
    pub fn wait(self) -> u64 {
        self.wait
    }

    /// Returns the monotonically increasing generation within that wait object.
    pub fn generation(self) -> u64 {
        self.generation
    }
}

/// One reusable stack mapping within its owning carrier.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
    ::core::cmp::PartialOrd,
    ::core::cmp::Ord,
    ::core::hash::Hash,
)]
pub struct StackId {
    carrier: CarrierId,
    local: u64,
}

impl StackId {
    pub(crate) fn new(carrier: CarrierId, local: u64) -> Self {
        Self { carrier, local }
    }

    /// Returns the carrier that owns this mapping for its entire lifetime.
    pub fn carrier(self) -> CarrierId {
        self.carrier
    }

    /// Returns the monotonically allocated carrier-local identity.
    pub fn local(self) -> u64 {
        self.local
    }
}

/// Sequenced evidence from one runtime.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
pub struct RuntimeEvent {
    sequence: EventSequence,
    runtime: RuntimeId,
    kind: RuntimeEventKind,
}

impl RuntimeEvent {
    pub(crate) fn new(sequence: u64, runtime: RuntimeId, kind: RuntimeEventKind) -> Self {
        Self {
            sequence: EventSequence::new(sequence),
            runtime,
            kind,
        }
    }

    /// Returns the runtime-wide publication sequence.
    pub fn sequence(self) -> EventSequence {
        self.sequence
    }

    /// Returns the process-unique runtime identity.
    pub fn runtime(self) -> RuntimeId {
        self.runtime
    }

    /// Returns the typed transition payload.
    pub fn kind(self) -> RuntimeEventKind {
        self.kind
    }
}

#[cfg(test)]
#[path = "event_test.rs"]
mod event_test;
