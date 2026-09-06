//! Owner-local useful mutex work, distinct from routing and native sleep evidence.

use crate::handoff_span::record;

/// Carrier-initiated mutex operations after the entry checkpoint. `try_lock`
/// calls and producers without a mounted carrier route are not counted. Stored
/// grants include offers to inactive generations, not unique future acquisitions.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::default::Default,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
pub struct MutexCounters {
    calls: u64,
    acquired: u64,
    failed_calls: u64,
    pub(crate) immediate: u64,
    pub(crate) recheck: u64,
    pub(crate) queued: u64,
    pub(crate) park_returns: u64,
    pub(crate) parks: u64,
    pub(crate) completed: u64,
    pub(crate) dropped: u64,
    pub(crate) removed: u64,
    pub(crate) abandoned: u64,
    attempts: u64,
    stored: u64,
    rejected: u64,
    same_owner: u64,
    other_owner: u64,
}

impl MutexCounters {
    /// Blocking API calls begun after their entry checkpoint.
    pub fn calls(&self) -> u64 {
        self.calls
    }
    /// Calls returning exclusive access, including immediately successful calls.
    pub fn acquired(&self) -> u64 {
        self.acquired
    }
    /// Calls exiting without exclusive access, including unwinding.
    pub fn failed_calls(&self) -> u64 {
        self.failed_calls
    }
    /// Acquisitions at the first resource claim, before wait attachment.
    pub fn immediate(&self) -> u64 {
        self.immediate
    }
    /// Acquisitions at the queue-locked resource recheck, without a ticket.
    pub fn recheck(&self) -> u64 {
        self.recheck
    }
    /// Outstanding FIFO tickets admitted, including subsequently cancelled tickets.
    pub fn queued(&self) -> u64 {
        self.queued
    }
    /// Successful permit-wait returns including the resume cancellation checkpoint.
    /// An immediate stored grant can return without an actual suspension.
    pub fn park_returns(&self) -> u64 {
        self.park_returns
    }
    /// Actual mutex-reason park crossings processed by the recipient owner kernel.
    pub fn parks(&self) -> u64 {
        self.parks
    }
    /// Tickets retired after taking their linear ownership capability.
    pub fn completed(&self) -> u64 {
        self.completed
    }
    /// Tickets retired without normal completion, including cancellation/unwind.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
    /// Dropped tickets removed while still queued, before resource selection.
    pub fn removed(&self) -> u64 {
        self.removed
    }
    /// Dropped selected tickets that return ownership for onward transfer.
    /// A rejected, already-dequeued ticket is neither removed nor abandoned.
    pub fn abandoned(&self) -> u64 {
        self.abandoned
    }
    /// Completed resource-offer attempts initiated on this carrier.
    pub fn attempts(&self) -> u64 {
        self.attempts
    }
    /// Offers storing a resource without selecting an active park generation.
    pub fn stored(&self) -> u64 {
        self.stored
    }
    /// Resource offers rejected by the selected/dead wait state.
    pub fn rejected(&self) -> u64 {
        self.rejected
    }
    /// Active grants to the source's owning hub, independent of queue choice.
    pub fn same_owner(&self) -> u64 {
        self.same_owner
    }
    /// Active grants to a different owning hub; not proof of recipient sleep.
    pub fn other_owner(&self) -> u64 {
        self.other_owner
    }
}

pub(crate) struct MutexCall {
    acquired: bool,
}

impl MutexCall {
    pub(crate) fn new() -> Self {
        record(|profile| profile.mutex.calls += 1);
        Self { acquired: false }
    }

    pub(crate) fn acquired(&mut self) {
        record(|profile| profile.mutex.acquired += 1);
        self.acquired = true;
    }
}

impl Drop for MutexCall {
    fn drop(&mut self) {
        if !self.acquired {
            record(|profile| profile.mutex.failed_calls += 1);
        }
    }
}

pub(crate) enum Grant {
    Stored,
    SameOwner,
    OtherOwner,
    Rejected,
}

pub(crate) fn grant(outcome: Grant) {
    record(|profile| {
        let counts = &mut profile.mutex;
        counts.attempts += 1;
        match outcome {
            Grant::Stored => counts.stored += 1,
            Grant::SameOwner => counts.same_owner += 1,
            Grant::OtherOwner => counts.other_owner += 1,
            Grant::Rejected => counts.rejected += 1,
        }
    });
}

#[cfg(test)]
#[path = "handoff_mutex_test.rs"]
mod handoff_mutex_test;
