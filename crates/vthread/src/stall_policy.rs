//! Explicit policy for observed scope inactivity, which does not prove deadlock.

use std::time::Duration;

/// Response to a timerless, entirely parked root scope while an OS owner waits.
/// External wake ownership cannot be inferred from quiescence. Disabled is the safe default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum StallPolicy {
    /// Wait for legitimate external progress without automatic inactivity recovery.
    #[default]
    Disabled,
    /// Retain one diagnostic per observed inactive interval without cancelling children.
    ReportAfter(Duration),
    /// Reclaim this scope after the given inactive interval; explicitly opt into data loss.
    AbortAfter(Duration),
}

impl StallPolicy {
    pub(crate) fn timeout(self) -> Option<Duration> {
        match self {
            Self::Disabled => None,
            Self::ReportAfter(timeout) | Self::AbortAfter(timeout) => Some(timeout),
        }
    }

    pub(crate) fn aborts(self) -> bool {
        matches!(self, Self::AbortAfter(_))
    }
}

#[cfg(test)]
#[path = "stall_policy_test.rs"]
mod stall_policy_test;
