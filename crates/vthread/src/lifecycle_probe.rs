//! Opt-in timing attribution for controlled scheduler lifecycle benchmarks.

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

/// Cumulative carrier lifecycle work observed by one runtime.
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
    stack_fiber_ns: u64,
    stack_fiber_operations: u64,
    reclaim_ns: u64,
    reclaim_operations: u64,
    completion_ns: u64,
    completion_operations: u64,
}

impl LifecycleProfile {
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

    /// Returns the component-wise increase since an earlier cumulative snapshot.
    pub fn checked_delta(self, earlier: Self) -> Option<Self> {
        Some(Self {
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
        })
    }
}

pub(crate) struct Recorder {
    stack_fiber_ns: AtomicU64,
    stack_fiber_operations: AtomicU64,
    reclaim_ns: AtomicU64,
    reclaim_operations: AtomicU64,
    completion_ns: AtomicU64,
    completion_operations: AtomicU64,
}

impl Recorder {
    pub(crate) fn new() -> Self {
        Self {
            stack_fiber_ns: AtomicU64::new(0),
            stack_fiber_operations: AtomicU64::new(0),
            reclaim_ns: AtomicU64::new(0),
            reclaim_operations: AtomicU64::new(0),
            completion_ns: AtomicU64::new(0),
            completion_operations: AtomicU64::new(0),
        }
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

    pub(crate) fn snapshot(&self) -> LifecycleProfile {
        LifecycleProfile {
            stack_fiber_ns: self.stack_fiber_ns.load(Ordering::Relaxed),
            stack_fiber_operations: self.stack_fiber_operations.load(Ordering::Relaxed),
            reclaim_ns: self.reclaim_ns.load(Ordering::Relaxed),
            reclaim_operations: self.reclaim_operations.load(Ordering::Relaxed),
            completion_ns: self.completion_ns.load(Ordering::Relaxed),
            completion_operations: self.completion_operations.load(Ordering::Relaxed),
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
