//! Read-only access to runtime observations; callers cannot construct internal state.

impl crate::RuntimeStats {
    /// Tasks accepted by the runtime.
    pub fn admitted(&self) -> u64 {
        self.admitted
    }
    /// Tasks that returned normally.
    pub fn completed(&self) -> u64 {
        self.completed
    }
    /// Tasks that panicked.
    pub fn panicked(&self) -> u64 {
        self.panicked
    }
    /// Total stack mounts.
    pub fn mounts(&self) -> u64 {
        self.mounts
    }
    /// Total cooperative yields.
    pub fn yields(&self) -> u64 {
        self.yields
    }
    /// Total modeled park operations.
    pub fn parks(&self) -> u64 {
        self.parks
    }
    /// Parked generations made runnable again.
    pub fn wakes(&self) -> u64 {
        self.wakes
    }
    /// Wake selections caused by monotonic deadlines.
    pub fn timeouts(&self) -> u64 {
        self.timeouts
    }
    /// Wake selections caused by explicit cancellation.
    pub fn cancelled(&self) -> u64 {
        self.cancelled
    }
    /// Wake selections caused by permanent close.
    pub fn closed(&self) -> u64 {
        self.closed
    }
    /// Carrier sleeps while waiting for the next timer.
    pub fn timer_sleeps(&self) -> u64 {
        self.timer_sleeps
    }
    /// Wake notices ignored after their generation was no longer parked.
    pub fn stale_wakes(&self) -> u64 {
        self.stale_wakes
    }
    /// Tasks discarded while recovering a stalled scope.
    pub fn aborted(&self) -> u64 {
        self.aborted
    }
    /// Spawn attempts rejected at capacity.
    pub fn rejected(&self) -> u64 {
        self.rejected
    }
}

impl crate::StackSnapshot {
    /// Stacks currently retained.
    pub fn cached(&self) -> usize {
        self.cached
    }
    /// Fresh stack mappings created.
    pub fn allocated(&self) -> u64 {
        self.allocated
    }
    /// Cached stacks reused.
    pub fn reused(&self) -> u64 {
        self.reused
    }
    /// Completed stacks retained.
    pub fn retained(&self) -> u64 {
        self.retained
    }
    /// Completed stacks discarded at the cache limit.
    pub fn discarded(&self) -> u64 {
        self.discarded
    }
}

#[cfg(test)]
#[path = "metrics_accessors_test.rs"]
mod metrics_accessors_test;
