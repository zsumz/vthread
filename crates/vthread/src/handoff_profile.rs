//! Bounded owner-local handoff evidence; clocks exist only in the opt-in build.

use std::time::Duration;

pub use crate::handoff_channel::{ChannelCounters, ChannelDirection};
pub use crate::handoff_mutex::MutexCounters;

/// Inclusive duration histogram bounds in nanoseconds; the last bin includes all
/// larger observations. Clock overhead is not subtracted.
pub const HANDOFF_DURATION_BOUNDS_NS: [u64; 10] = [
    1_000,
    5_000,
    10_000,
    25_000,
    50_000,
    100_000,
    250_000,
    1_000_000,
    10_000_000,
    u64::MAX,
];

/// A measured region, not a disjoint CPU-time category. Nested regions overlap;
/// elapsed time includes preemption and must not be summed as CPU consumption.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
pub enum HandoffStage {
    /// Complete idle-driver invocation, including completion flushing and observation.
    IdleEpisode,
    /// Polling that ends with a visible start or wake, not proof of a valid dispatch.
    PollWork,
    /// Polling that ends with a changed control epoch but no observed task work.
    PollControl,
    /// Polling that exhausts the unchanged bounded probe budget.
    PollExhausted,
    /// Idle snapshot publication before the wait API, including capacity scans.
    IdlePublication,
    /// Predicate-backed wait API, including locking, arming and any native waits.
    WaitApi,
    /// A condition-variable invocation including reacquisition of its native lock.
    /// This does not prove that the OS descheduled the thread.
    NativeWait,
    /// Selected wait claim through route publication and final claim publication.
    WakePublication,
    /// A finish path that observed a claim/binding phase, through its exit.
    ClaimFinish,
    /// Acquisition of the channel metadata lock, excluding its held section.
    ChannelLock,
    /// Channel metadata critical section, excluding unlock and lock acquisition.
    ChannelHeld,
}

impl HandoffStage {
    /// All measured regions in stable histogram order.
    pub const ALL: [Self; 11] = [
        Self::IdleEpisode,
        Self::PollWork,
        Self::PollControl,
        Self::PollExhausted,
        Self::IdlePublication,
        Self::WaitApi,
        Self::NativeWait,
        Self::WakePublication,
        Self::ClaimFinish,
        Self::ChannelLock,
        Self::ChannelHeld,
    ];
}

/// Fixed-size elapsed-time distribution. Counts are cumulative; total nanoseconds
/// saturate at `u64::MAX`. Observations are diagnostic, not headline latency data.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::default::Default,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
pub struct HandoffDuration {
    count: u64,
    total_ns: u64,
    maximum_ns: u64,
    bins: [u64; 10],
}

impl HandoffDuration {
    pub(crate) fn record(&mut self, elapsed: Duration) {
        let nanos = elapsed.as_nanos().min(u128::from(u64::MAX)) as u64;
        let bin = HANDOFF_DURATION_BOUNDS_NS.partition_point(|bound| *bound < nanos);
        self.count += 1;
        self.total_ns = self.total_ns.saturating_add(nanos);
        self.maximum_ns = self.maximum_ns.max(nanos);
        self.bins[bin] += 1;
    }

    /// Number of completed measured regions.
    pub fn count(&self) -> u64 {
        self.count
    }
    /// Saturating cumulative elapsed nanoseconds, not CPU time.
    pub fn total_ns(&self) -> u64 {
        self.total_ns
    }
    /// Largest individual elapsed duration in nanoseconds.
    pub fn maximum_ns(&self) -> u64 {
        self.maximum_ns
    }
    /// Counts corresponding to [`HANDOFF_DURATION_BOUNDS_NS`].
    pub fn bins(&self) -> &[u64; 10] {
        &self.bins
    }
}

/// Cumulative evidence attributed to the executing owner carrier. Native callers
/// without a mounted carrier route are not counted. No new shared counter atomics
/// or per-observation allocations are used. Active snapshots can lag; shutdown
/// publishes final totals. Enabled clocks, TLS accesses and larger snapshots affect
/// scheduling, so this feature cannot supply uninstrumented performance claims.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::default::Default,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
pub struct HandoffProfile {
    durations: [HandoffDuration; 11],
    pub(crate) channels: [ChannelCounters; 2],
    pub(crate) mutex: MutexCounters,
    pub(crate) local_publications: u64,
    pub(crate) remote_publications: u64,
}

impl HandoffProfile {
    pub(crate) fn record(&mut self, stage: HandoffStage, elapsed: Duration) {
        self.durations[stage as usize].record(elapsed);
    }

    /// Elapsed distribution for one possibly overlapping region.
    pub fn duration(&self, stage: HandoffStage) -> &HandoffDuration {
        &self.durations[stage as usize]
    }
    /// Channel operations and notifications initiated on this carrier.
    /// Actual park crossings are counted on the same non-migrating owner.
    pub fn channel(&self, direction: ChannelDirection) -> &ChannelCounters {
        &self.channels[direction as usize]
    }
    /// Useful mutex acquisitions, ticket lifetimes and selected owner identities.
    /// Source-side grants and recipient-side receipts may belong to different carriers.
    pub fn mutex(&self) -> &MutexCounters {
        &self.mutex
    }
    /// Selected notices routed through the source carrier's local queue.
    pub fn local_publications(&self) -> u64 {
        self.local_publications
    }
    /// Selected notices routed through a shared hub, including forced shared routing.
    /// These are not necessarily distinct-carrier destinations or successful inserts.
    pub fn remote_publications(&self) -> u64 {
        self.remote_publications
    }
}

#[cfg(test)]
#[path = "handoff_profile_test.rs"]
mod handoff_profile_test;
