//! Single terminal commit after task stack ownership has ended.

use super::Shared;
use crate::{TaskFailure, TaskStatus, signal::lock, task::SharedTaskRecord};
use std::sync::Arc;

impl Shared {
    pub(crate) fn complete(&self, record: &SharedTaskRecord, failure: Option<TaskFailure>) {
        let mut state = lock(&self.state);
        let mut record = lock(record);
        if record.status.is_terminal() {
            return;
        }
        record.failure = failure;
        record.deadline = None;
        record.status = if failure.is_some() {
            TaskStatus::Aborted
        } else if record.panic.is_some() {
            TaskStatus::Panicked
        } else {
            TaskStatus::Completed
        };
        if let Some(scope) = state.scopes.get_mut(&record.scope) {
            scope.activity = scope.activity.wrapping_add(1);
            match record.status {
                TaskStatus::Aborted => scope.aborted += 1,
                TaskStatus::Panicked => scope.panicked += 1,
                _ => scope.completed += 1,
            }
        }
        let completion = Arc::clone(&record.completion);
        #[cfg(feature = "runtime-evidence")]
        let terminal = (record.id, record.status, failure);
        state.active -= 1;
        state.loads[record.carrier.0] -= 1;
        drop(record);
        drop(state);
        #[cfg(feature = "runtime-evidence")]
        self.record_terminal(terminal.0, terminal.1, terminal.2);
        completion.complete();
        self.changed.notify();
    }
}

#[cfg(test)]
#[path = "control_completion_test.rs"]
mod control_completion_test;
