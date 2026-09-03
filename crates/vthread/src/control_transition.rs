//! Task metadata transitions with optional exact stall-activity publication.

use super::Shared;
use crate::signal::lock;
use crate::task::{SharedTaskRecord, TaskRecord};

impl Shared {
    pub(crate) fn transition<R>(
        &self,
        record: &SharedTaskRecord,
        update: impl FnOnce(&mut TaskRecord) -> R,
    ) -> R {
        if self.config.stall_policy().timeout().is_none() {
            return update(&mut record.lock());
        }
        let mut state = lock(&self.state);
        let mut task = record.lock();
        let result = update(&mut task);
        if let Some(scope) = state.scopes.get_mut(&task.scope) {
            scope.activity = scope.activity.wrapping_add(1);
        }
        drop(task);
        drop(state);
        self.changed.notify();
        result
    }
}

#[cfg(test)]
#[path = "control_transition_test.rs"]
mod control_transition_test;
