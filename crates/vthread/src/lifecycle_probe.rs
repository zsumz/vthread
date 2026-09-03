//! Opt-in timing attribution for controlled scheduler lifecycle benchmarks.

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

/// Cumulative task lifecycle work observed by one runtime.
///
/// A snapshot taken while tasks are active is weakly consistent across phases. Take snapshots
/// between drained scopes when using [`Self::checked_delta`] for benchmark attribution.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
pub struct LifecycleProfile {
    reservation_ns: u64,
    envelope_ns: u64,
    inbox_ns: u64,
    admission_operations: u64,
    stack_fiber_ns: u64,
    stack_fiber_operations: u64,
    reclaim_ns: u64,
    reclaim_operations: u64,
    completion_ns: u64,
    completion_operations: u64,
    record_retirement_ns: u64,
    record_retirement_operations: u64,
}

impl LifecycleProfile {
    /// Nanoseconds spent reserving task identity, policy, capacity, and diagnostics.
    pub fn reservation_nanoseconds(self) -> u64 {
        self.reservation_ns
    }

    /// Nanoseconds spent allocating the typed result cell and executable envelope.
    pub fn envelope_nanoseconds(self) -> u64 {
        self.envelope_ns
    }

    /// Nanoseconds spent transferring accepted envelopes into carrier inboxes.
    pub fn inbox_nanoseconds(self) -> u64 {
        self.inbox_ns
    }

    /// Accepted tasks included in each producer-side admission duration.
    pub fn admission_operations(self) -> u64 {
        self.admission_operations
    }

    /// Nanoseconds spent acquiring stacks and materializing owned fibers and task contexts.
    pub fn stack_fiber_nanoseconds(self) -> u64 {
        self.stack_fiber_ns
    }

    /// Owned tasks included in the stack/fiber materialization duration.
    pub fn stack_fiber_operations(self) -> u64 {
        self.stack_fiber_operations
    }

    /// Nanoseconds spent reclaiming stacks, execution contexts, and task-slab slots.
    pub fn reclaim_nanoseconds(self) -> u64 {
        self.reclaim_ns
    }

    /// Owned tasks included in the reclaim duration.
    pub fn reclaim_operations(self) -> u64 {
        self.reclaim_operations
    }

    /// Nanoseconds spent committing and publishing terminal task state.
    pub fn completion_nanoseconds(self) -> u64 {
        self.completion_ns
    }

    /// Terminal tasks included in the completion duration.
    pub fn completion_operations(self) -> u64 {
        self.completion_operations
    }

    /// Nanoseconds spent retiring task diagnostics after their owner scope drains.
    pub fn record_retirement_nanoseconds(self) -> u64 {
        self.record_retirement_ns
    }

    /// Task records included in scope-side diagnostic retirement.
    pub fn record_retirement_operations(self) -> u64 {
        self.record_retirement_operations
    }

    /// Returns the component-wise increase since an earlier cumulative snapshot.
    pub fn checked_delta(self, earlier: Self) -> Option<Self> {
        Some(Self {
            reservation_ns: self.reservation_ns.checked_sub(earlier.reservation_ns)?,
            envelope_ns: self.envelope_ns.checked_sub(earlier.envelope_ns)?,
            inbox_ns: self.inbox_ns.checked_sub(earlier.inbox_ns)?,
            admission_operations: self
                .admission_operations
                .checked_sub(earlier.admission_operations)?,
            stack_fiber_ns: self.stack_fiber_ns.checked_sub(earlier.stack_fiber_ns)?,
            stack_fiber_operations: self
                .stack_fiber_operations
                .checked_sub(earlier.stack_fiber_operations)?,
            reclaim_ns: self.reclaim_ns.checked_sub(earlier.reclaim_ns)?,
            reclaim_operations: self
                .reclaim_operations
                .checked_sub(earlier.reclaim_operations)?,
            completion_ns: self.completion_ns.checked_sub(earlier.completion_ns)?,
            completion_operations: self
                .completion_operations
                .checked_sub(earlier.completion_operations)?,
            record_retirement_ns: self
                .record_retirement_ns
                .checked_sub(earlier.record_retirement_ns)?,
            record_retirement_operations: self
                .record_retirement_operations
                .checked_sub(earlier.record_retirement_operations)?,
        })
    }
}

pub(crate) struct Recorder {
    reservation_ns: AtomicU64,
    envelope_ns: AtomicU64,
    inbox_ns: AtomicU64,
    admission_operations: AtomicU64,
    stack_fiber_ns: AtomicU64,
    stack_fiber_operations: AtomicU64,
    reclaim_ns: AtomicU64,
    reclaim_operations: AtomicU64,
    completion_ns: AtomicU64,
    completion_operations: AtomicU64,
    record_retirement_ns: AtomicU64,
    record_retirement_operations: AtomicU64,
}

impl Recorder {
    pub(crate) fn new() -> Self {
        Self {
            reservation_ns: AtomicU64::new(0),
            envelope_ns: AtomicU64::new(0),
            inbox_ns: AtomicU64::new(0),
            admission_operations: AtomicU64::new(0),
            stack_fiber_ns: AtomicU64::new(0),
            stack_fiber_operations: AtomicU64::new(0),
            reclaim_ns: AtomicU64::new(0),
            reclaim_operations: AtomicU64::new(0),
            completion_ns: AtomicU64::new(0),
            completion_operations: AtomicU64::new(0),
            record_retirement_ns: AtomicU64::new(0),
            record_retirement_operations: AtomicU64::new(0),
        }
    }

    pub(crate) fn record_admission(
        &self,
        reservation: Duration,
        envelope: Duration,
        inbox: Duration,
    ) {
        add_duration(&self.reservation_ns, reservation);
        add_duration(&self.envelope_ns, envelope);
        add_duration(&self.inbox_ns, inbox);
        add(&self.admission_operations, 1);
    }

    pub(crate) fn record_stack_fiber(&self, elapsed: Duration) {
        add_duration(&self.stack_fiber_ns, elapsed);
        add(&self.stack_fiber_operations, 1);
    }

    pub(crate) fn record_reclaim(&self, elapsed: Duration) {
        add_duration(&self.reclaim_ns, elapsed);
        add(&self.reclaim_operations, 1);
    }

    pub(crate) fn record_completion(&self, elapsed: Duration, operations: usize) {
        add_duration(&self.completion_ns, elapsed);
        if operations != 0 {
            add(
                &self.completion_operations,
                u64::try_from(operations).unwrap_or(u64::MAX),
            );
        }
    }

    pub(crate) fn record_retirement(&self, elapsed: Duration, operations: usize) {
        add_duration(&self.record_retirement_ns, elapsed);
        if operations != 0 {
            add(
                &self.record_retirement_operations,
                u64::try_from(operations).unwrap_or(u64::MAX),
            );
        }
    }

    pub(crate) fn snapshot(&self) -> LifecycleProfile {
        LifecycleProfile {
            reservation_ns: self.reservation_ns.load(Ordering::Relaxed),
            envelope_ns: self.envelope_ns.load(Ordering::Relaxed),
            inbox_ns: self.inbox_ns.load(Ordering::Relaxed),
            admission_operations: self.admission_operations.load(Ordering::Relaxed),
            stack_fiber_ns: self.stack_fiber_ns.load(Ordering::Relaxed),
            stack_fiber_operations: self.stack_fiber_operations.load(Ordering::Relaxed),
            reclaim_ns: self.reclaim_ns.load(Ordering::Relaxed),
            reclaim_operations: self.reclaim_operations.load(Ordering::Relaxed),
            completion_ns: self.completion_ns.load(Ordering::Relaxed),
            completion_operations: self.completion_operations.load(Ordering::Relaxed),
            record_retirement_ns: self.record_retirement_ns.load(Ordering::Relaxed),
            record_retirement_operations: self.record_retirement_operations.load(Ordering::Relaxed),
        }
    }
}

fn add_duration(total: &AtomicU64, elapsed: Duration) {
    add(total, u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX));
}

fn add(total: &AtomicU64, value: u64) {
    let _ = total.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_add(value))
    });
}

#[cfg(test)]
#[path = "lifecycle_probe_test.rs"]
mod lifecycle_probe_test;
