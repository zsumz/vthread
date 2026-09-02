//! Single terminal commit after task stack ownership has ended.

use super::Shared;
use crate::{TaskFailure, TaskStatus, signal::lock, task::SharedTaskRecord};
impl Shared {
    pub(crate) fn complete(&self, record: &SharedTaskRecord, failure: Option<TaskFailure>) {
        let mut state = lock(&self.state);
        let mut task_record = record.lock();
        if task_record.status.is_terminal() {
            return;
        }
        task_record.failure = failure;
        task_record.deadline = None;
        task_record.status = if failure.is_some() {
            TaskStatus::Aborted
        } else if task_record.panic.is_some() {
            TaskStatus::Panicked
        } else {
            TaskStatus::Completed
        };
        if let Some(scope) = state.scopes.get_mut(&task_record.scope) {
            scope.activity = scope.activity.wrapping_add(1);
            match task_record.status {
                TaskStatus::Aborted => scope.aborted += 1,
                TaskStatus::Panicked => scope.panicked += 1,
                _ => scope.completed += 1,
            }
        }
        #[cfg(feature = "runtime-evidence")]
        let terminal = (task_record.id, task_record.status, failure);
        state.active -= 1;
        state.loads[task_record.carrier.0] -= 1;
        drop(task_record);
        drop(state);
        #[cfg(feature = "runtime-evidence")]
        self.record_terminal(terminal.0, terminal.1, terminal.2);
        record.completion().complete();
        self.changed.notify();
    }
}

#[cfg(test)]
#[path = "control_completion_test.rs"]
mod control_completion_test;
