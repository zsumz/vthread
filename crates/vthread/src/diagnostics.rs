//! Runtime and stack-pool diagnostics.

use crate::TaskSnapshot;

/// Cumulative scheduler counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuntimeStats {
    /// Tasks accepted by the runtime.
    pub spawned: u64,
    /// Tasks that returned normally.
    pub completed: u64,
    /// Tasks that panicked.
    pub panicked: u64,
    /// Total stack mounts.
    pub mounts: u64,
    /// Total cooperative yields.
    pub yields: u64,
    /// Spawn attempts rejected at capacity.
    pub rejected: u64,
}

/// Bounded stack-cache counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StackSnapshot {
    /// Stacks currently retained.
    pub cached: usize,
    /// Fresh stack mappings created.
    pub allocated: u64,
    /// Cached stacks reused.
    pub reused: u64,
    /// Completed stacks retained.
    pub retained: u64,
    /// Completed stacks discarded at the cache limit.
    pub discarded: u64,
}

impl From<vthread_stack::StackPoolSnapshot> for StackSnapshot {
    fn from(snapshot: vthread_stack::StackPoolSnapshot) -> Self {
        Self {
            cached: snapshot.cached,
            allocated: snapshot.allocated,
            reused: snapshot.reused,
            retained: snapshot.retained,
            discarded: snapshot.discarded,
        }
    }
}

/// Point-in-time runtime state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeSnapshot {
    /// Number of live tasks.
    pub active: usize,
    /// Number of tasks waiting in the run queue.
    pub runnable: usize,
    /// Cumulative scheduler counters.
    pub stats: RuntimeStats,
    /// Stack-cache counters.
    pub stacks: StackSnapshot,
    /// Task records retained by the active scope.
    pub tasks: Vec<TaskSnapshot>,
}

#[cfg(test)]
#[path = "diagnostics_test.rs"]
mod diagnostics_test;
