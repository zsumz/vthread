//! Read-only access to runtime observations; callers cannot construct internal state.

impl crate::RuntimeSnapshot {
    /// Latest inert, bounded scope report; original error sources remain caller-owned.
    pub fn last_scope_failure(&self) -> Option<&crate::scope_failure_report::ScopeFailureReport> {
        self.last_scope_failure.as_deref()
    }
    /// Bounded terminal component failures, retained through shutdown.
    pub fn failures(&self) -> &crate::ThreadFailures {
        &self.failures
    }
    /// Current shutdown progress, including waits beyond task and native-job completion.
    pub fn shutdown_phase(&self) -> crate::ShutdownPhase {
        self.shutdown_phase
    }
    /// Whether new root scopes and task admissions are accepted (subject to capacity).
    pub fn accepting(&self) -> bool {
        self.accepting
    }
    /// Most recent stalled scope; only one bounded report is retained per runtime.
    pub fn last_stall(&self) -> Option<&crate::StallSnapshot> {
        self.last_stall.as_ref()
    }
    /// Readiness registration and delegated native-work bounds and activity.
    pub fn services(&self) -> &crate::ServiceSnapshot {
        &self.services
    }
    /// Per-carrier health and ownership counters.
    pub fn carriers(&self) -> &[crate::CarrierSnapshot] {
        &self.carriers
    }
    /// Number of live tasks.
    pub fn active(&self) -> usize {
        self.active
    }
    /// Number of tasks waiting in the run queue.
    pub fn runnable(&self) -> usize {
        self.runnable
    }
    /// Number of tasks parked on wait generations.
    pub fn parked(&self) -> usize {
        self.parked
    }
    /// Number of active monotonic timers.
    pub fn timers(&self) -> usize {
        self.timers
    }
    /// Cumulative scheduler counters.
    pub fn stats(&self) -> crate::RuntimeStats {
        self.stats
    }
    /// Stack-cache counters.
    pub fn stacks(&self) -> crate::StackSnapshot {
        self.stacks
    }
    /// Task records retained by the active scope.
    pub fn tasks(&self) -> &[crate::TaskSnapshot] {
        &self.tasks
    }
}

impl crate::CarrierSnapshot {
    /// Stable runtime-local identity.
    pub fn id(&self) -> crate::CarrierId {
        self.id
    }
    /// Current carrier health.
    pub fn status(&self) -> crate::CarrierStatus {
        self.status
    }
    /// Tasks with retained stacks or unstarted admission.
    pub fn active(&self) -> usize {
        self.active
    }
    /// Local runnable stacks.
    pub fn runnable(&self) -> usize {
        self.runnable
    }
    /// Local parked stacks.
    pub fn parked(&self) -> usize {
        self.parked
    }
    /// Active monotonic timers.
    pub fn timers(&self) -> usize {
        self.timers
    }
    /// Unstarted packets waiting in the bounded inbox.
    pub fn pending_starts(&self) -> usize {
        self.pending_starts
    }
    /// Selected wake notices waiting in reserved slots.
    pub fn pending_wakes(&self) -> usize {
        self.pending_wakes
    }
    /// Cumulative carrier counters.
    pub fn stats(&self) -> crate::RuntimeStats {
        self.stats
    }
    /// Carrier-local stack cache.
    pub fn stacks(&self) -> crate::StackSnapshot {
        self.stacks
    }
}

impl crate::StallSnapshot {
    /// Explicit policy that caused this observation; reporting alone never cancels work.
    pub fn policy(&self) -> crate::StallPolicy {
        self.policy
    }
    /// Root scope selected for recovery.
    pub fn scope(&self) -> crate::diagnostics::ScopeId {
        crate::diagnostics::ScopeId::new(self.scope)
    }
    /// Monotonic detection time.
    pub fn detected_at(&self) -> std::time::Instant {
        self.detected_at
    }
    /// Observed quiescent interval before recovery.
    pub fn quiescent_for(&self) -> std::time::Duration {
        self.quiescent_for
    }
    /// Live tasks before abort, bounded by the configured task admission limit.
    pub fn tasks(&self) -> &[crate::TaskSnapshot] {
        &self.tasks
    }
}

#[cfg(test)]
#[path = "diagnostics_accessors_test.rs"]
mod diagnostics_accessors_test;
