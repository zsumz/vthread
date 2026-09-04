//! Compact atomic encoding for one reusable wait generation.

use super::{ResourceSelection, WakeCause};

const PHASE_MASK: u64 = 0x0f;
const PERMIT: u64 = 1 << 4;
const CLOSED: u64 = 1 << 5;
const RESOURCE_SHIFT: u32 = 6;
const RESOURCE_MASK: u64 = 0x03 << RESOURCE_SHIFT;
const FALLBACK_HUB: u64 = 1 << 8;
const GENERATION_SHIFT: u32 = 9;
pub(super) const MAX_GENERATION: u64 = u64::MAX >> GENERATION_SHIFT;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum Phase {
    Idle = 0,
    Binding = 1,
    Active = 2,
    ClaimReady = 3,
    SelectedReady = 4,
    ClaimTimedOut = 5,
    SelectedTimedOut = 6,
    ClaimCancelled = 7,
    SelectedCancelled = 8,
    ClaimInheritedCancelled = 9,
    SelectedInheritedCancelled = 10,
    ClaimClosed = 11,
    SelectedClosed = 12,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WaitWord(u64);

impl WaitWord {
    pub(super) const fn initial() -> Self {
        Self(0)
    }

    pub(super) const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    pub(super) const fn raw(self) -> u64 {
        self.0
    }

    pub(super) fn phase(self) -> Phase {
        match self.0 & PHASE_MASK {
            0 => Phase::Idle,
            1 => Phase::Binding,
            2 => Phase::Active,
            3 => Phase::ClaimReady,
            4 => Phase::SelectedReady,
            5 => Phase::ClaimTimedOut,
            6 => Phase::SelectedTimedOut,
            7 => Phase::ClaimCancelled,
            8 => Phase::SelectedCancelled,
            9 => Phase::ClaimInheritedCancelled,
            10 => Phase::SelectedInheritedCancelled,
            11 => Phase::ClaimClosed,
            12 => Phase::SelectedClosed,
            _ => unreachable!("invalid wait phase"),
        }
    }

    pub(super) const fn generation(self) -> u64 {
        self.0 >> GENERATION_SHIFT
    }

    pub(super) const fn has_permit(self) -> bool {
        self.0 & PERMIT != 0
    }

    pub(super) const fn is_closed(self) -> bool {
        self.0 & CLOSED != 0
    }

    pub(super) const fn uses_fallback_hub(self) -> bool {
        self.0 & FALLBACK_HUB != 0
    }

    pub(super) fn resource(self) -> Option<ResourceSelection> {
        match (self.0 & RESOURCE_MASK) >> RESOURCE_SHIFT {
            0 => None,
            1 => Some(ResourceSelection::Permit),
            2 => Some(ResourceSelection::Broadcast),
            _ => unreachable!("invalid wait resource"),
        }
    }

    pub(super) fn selected_cause(self) -> Option<WakeCause> {
        match self.phase() {
            Phase::SelectedReady => Some(WakeCause::Ready),
            Phase::SelectedTimedOut => Some(WakeCause::TimedOut),
            Phase::SelectedCancelled => Some(WakeCause::Cancelled),
            Phase::SelectedInheritedCancelled => Some(WakeCause::InheritedCancelled),
            Phase::SelectedClosed => Some(WakeCause::Closed),
            _ => None,
        }
    }

    pub(super) fn is_claimed(self) -> bool {
        matches!(
            self.phase(),
            Phase::ClaimReady
                | Phase::ClaimTimedOut
                | Phase::ClaimCancelled
                | Phase::ClaimInheritedCancelled
                | Phase::ClaimClosed
        )
    }

    pub(super) fn with_phase(self, phase: Phase) -> Self {
        Self((self.0 & !PHASE_MASK) | phase as u64)
    }

    pub(super) fn with_generation(self, generation: u64) -> Self {
        #[cfg(debug_assertions)]
        assert!(generation <= MAX_GENERATION);
        Self((self.0 & ((1 << GENERATION_SHIFT) - 1)) | (generation << GENERATION_SHIFT))
    }

    pub(super) fn with_permit(self, permit: bool) -> Self {
        Self(if permit {
            self.0 | PERMIT
        } else {
            self.0 & !PERMIT
        })
    }

    pub(super) fn with_closed(self, closed: bool) -> Self {
        Self(if closed {
            self.0 | CLOSED
        } else {
            self.0 & !CLOSED
        })
    }

    pub(super) fn with_fallback_hub(self, fallback: bool) -> Self {
        Self(if fallback {
            self.0 | FALLBACK_HUB
        } else {
            self.0 & !FALLBACK_HUB
        })
    }

    pub(super) fn with_resource(self, resource: Option<ResourceSelection>) -> Self {
        let encoded = match resource {
            None => 0,
            Some(ResourceSelection::Permit) => 1,
            Some(ResourceSelection::Broadcast) => 2,
        };
        Self((self.0 & !RESOURCE_MASK) | (encoded << RESOURCE_SHIFT))
    }

    pub(super) fn claimed(self, cause: WakeCause) -> Self {
        self.with_phase(match cause {
            WakeCause::Ready => Phase::ClaimReady,
            WakeCause::TimedOut => Phase::ClaimTimedOut,
            WakeCause::Cancelled => Phase::ClaimCancelled,
            WakeCause::InheritedCancelled => Phase::ClaimInheritedCancelled,
            WakeCause::Closed => Phase::ClaimClosed,
        })
    }

    pub(super) fn publish_claim(self) -> Self {
        self.with_phase(match self.phase() {
            Phase::ClaimReady => Phase::SelectedReady,
            Phase::ClaimTimedOut => Phase::SelectedTimedOut,
            Phase::ClaimCancelled => Phase::SelectedCancelled,
            Phase::ClaimInheritedCancelled => Phase::SelectedInheritedCancelled,
            Phase::ClaimClosed => Phase::SelectedClosed,
            _ => unreachable!("only a claimed wait can be published"),
        })
    }

    pub(super) fn retire(self) -> Self {
        self.with_phase(Phase::Idle).with_fallback_hub(false)
    }
}

#[cfg(test)]
#[path = "wait_state_test.rs"]
mod wait_state_test;
