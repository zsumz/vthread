//! Opt-in, bounded runtime evidence for qualification and exact replay.
//!
//! Evidence producers never run consumer code and never wait for consumer capacity.
//! Events from concurrent producers may arrive out of order; their runtime-wide sequence
//! is authoritative. Any full or disconnected buffer is reported as incomplete evidence.

mod emitter;
mod event;
mod recorder;
mod stream;
pub(crate) use emitter::Emitter;
pub use event::{
    EventSequence, EvidenceWakeCause, QueueKind, RuntimeEvent, RuntimeEventKind, StackDisposition,
    StackId, TaskOutcome, TimerRetirement, WaitKey, WakeOrigin, WakeRejection,
};
pub(crate) use recorder::{Recorder, bounded};
pub use stream::{EvidenceRecvError, EvidenceStatus, EvidenceStream};

/// Evidence schema emitted by this release candidate.
pub const SCHEMA_VERSION: u16 = 1;

/// Capabilities supported by one evidence stream.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
pub struct EvidenceCapabilities(u64);

impl EvidenceCapabilities {
    /// Runtime-wide event sequence numbers.
    pub const TOTAL_ORDER: Self = Self(1 << 0);
    /// Scope open and close transitions.
    pub const SCOPE_LIFECYCLE: Self = Self(1 << 1);
    /// Task admission, mount, suspension and terminal transitions.
    pub const TASK_LIFECYCLE: Self = Self(1 << 2);
    /// Exact reusable wait identities and generations.
    pub const WAIT_GENERATIONS: Self = Self(1 << 3);
    /// Wake offers, sole-winner selection and typed rejection.
    pub const WAKE_SELECTION: Self = Self(1 << 4);
    /// Reusable stack mapping checkout and release identities.
    pub const STACK_IDENTITIES: Self = Self(1 << 5);
    /// Exact timer registration and retirement transitions.
    pub const TIMER_LIFECYCLE: Self = Self(1 << 6);
    /// Bounded admission and wake queue depths.
    pub const QUEUE_DEPTHS: Self = Self(1 << 7);
    /// Runtime shutdown phase transitions.
    pub const SHUTDOWN_LIFECYCLE: Self = Self(1 << 8);
    /// Repeated mounts include the immutable owner carrier.
    pub const CARRIER_AFFINITY: Self = Self(1 << 9);
    /// A generation-bound stale-wake probe is compiled in.
    pub const STALE_WAKE_PROBE: Self = Self(1 << 10);
    /// Every wake offer identifies its carrier or external origin.
    pub const WAKE_ORIGINS: Self = Self(1 << 11);

    fn runtime() -> Self {
        let base = Self::TOTAL_ORDER.0
            | Self::SCOPE_LIFECYCLE.0
            | Self::TASK_LIFECYCLE.0
            | Self::WAIT_GENERATIONS.0
            | Self::WAKE_SELECTION.0
            | Self::STACK_IDENTITIES.0
            | Self::TIMER_LIFECYCLE.0
            | Self::QUEUE_DEPTHS.0
            | Self::SHUTDOWN_LIFECYCLE.0
            | Self::CARRIER_AFFINITY.0
            | Self::WAKE_ORIGINS.0;
        #[cfg(feature = "qualification")]
        let base = base | Self::STALE_WAKE_PROBE.0;
        Self(base)
    }

    /// Returns whether every requested capability is present.
    pub fn contains(self, requested: Self) -> bool {
        self.0 & requested.0 == requested.0
    }

    /// Returns the stable bit representation for trace headers.
    pub fn bits(self) -> u64 {
        self.0
    }
}

impl std::ops::BitOr for EvidenceCapabilities {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for EvidenceCapabilities {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[cfg(test)]
#[path = "evidence_test.rs"]
mod evidence_test;
