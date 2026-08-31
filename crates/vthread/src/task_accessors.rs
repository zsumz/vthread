//! Read-only access to runtime observations; callers cannot construct internal state.

impl crate::TaskSnapshot {
    /// Task identity.
    pub fn id(&self) -> crate::TaskId {
        self.id
    }
    /// User-supplied task name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Placement owner; the stack is created and always resumed on this carrier.
    pub fn carrier(&self) -> crate::CarrierId {
        self.carrier
    }
    /// Current park deadline, if any.
    pub fn deadline(&self) -> Option<std::time::Instant> {
        self.deadline
    }
    /// Earliest inherited scope deadline.
    pub fn inherited_deadline(&self) -> Option<std::time::Instant> {
        self.inherited_deadline
    }
    /// Owning root scope identity.
    pub fn scope(&self) -> crate::diagnostics::ScopeId {
        crate::diagnostics::ScopeId::new(self.scope)
    }
    /// Parent virtual thread for a borrowed local child.
    pub fn parent(&self) -> Option<crate::TaskId> {
        self.parent
    }
    /// Whether cancellation was requested by this task or an ancestor.
    pub fn cancellation_requested(&self) -> bool {
        self.cancellation_requested
    }
    /// Reclamation failure, if the task was aborted.
    pub fn failure(&self) -> Option<crate::TaskFailure> {
        self.failure
    }
    /// Current state.
    pub fn status(&self) -> crate::TaskStatus {
        self.status
    }
    /// Number of times the task stack was mounted.
    pub fn mounts(&self) -> u64 {
        self.mounts
    }
    /// Number of cooperative yields.
    pub fn yields(&self) -> u64 {
        self.yields
    }
    /// Number of modeled park operations.
    pub fn parks(&self) -> u64 {
        self.parks
    }
    /// Most recent typed suspension boundary, if any.
    pub fn last_suspension(&self) -> Option<crate::SuspensionReason> {
        self.last_suspension
    }
    /// Most recent selected wake reason, if any.
    pub fn last_wake(&self) -> Option<crate::WakeReason> {
        self.last_wake
    }
    /// Whether a join observed the outcome.
    pub fn outcome_observed(&self) -> bool {
        self.outcome_observed
    }
}

#[cfg(test)]
#[path = "task_accessors_test.rs"]
mod task_accessors_test;
