//! Supervisor-specific selection over a retained runtime observation.

use crate::diagnostics::{RuntimeSnapshot, ScopeId, TaskSnapshot};

/// A timed-out supervisor's identity and weakly consistent runtime observation.
/// `tasks()` includes retained terminal records as well as unfinished owned work.
#[derive(Clone, Debug)]
pub struct SupervisorTimeout {
    supervisor_id: ScopeId,
    snapshot: Box<RuntimeSnapshot>,
}

impl SupervisorTimeout {
    pub(crate) fn new(supervisor_id: ScopeId, snapshot: RuntimeSnapshot) -> Self {
        Self {
            supervisor_id,
            snapshot: Box::new(snapshot),
        }
    }

    /// The structured owner whose shutdown timed out.
    pub fn supervisor_id(&self) -> ScopeId {
        self.supervisor_id
    }

    /// The complete runtime observation, including other owners and shared services.
    pub fn runtime_snapshot(&self) -> &RuntimeSnapshot {
        &self.snapshot
    }

    /// Retained tasks belonging to this supervisor, without allocating another snapshot.
    pub fn tasks(&self) -> impl Iterator<Item = &TaskSnapshot> {
        self.snapshot
            .tasks()
            .iter()
            .filter(|task| task.scope() == self.supervisor_id)
    }
}

#[cfg(test)]
#[path = "supervisor_timeout_test.rs"]
mod supervisor_timeout_test;
