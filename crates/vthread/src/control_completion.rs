//! Single terminal commit after task stack ownership has ended.

use super::Shared;
use crate::{TaskFailure, TaskStatus, signal::lock, task::SharedTaskRecord};
use std::sync::atomic::Ordering;

#[derive(Clone, Copy)]
pub(crate) struct CompletionUpdate {
    id: crate::TaskId,
    pub(crate) scope: u64,
    carrier: crate::CarrierId,
    pub(crate) status: TaskStatus,
}

impl Shared {
    pub(crate) fn complete(&self, record: &SharedTaskRecord, failure: Option<TaskFailure>) {
        let Some(completion) = self.prepare_completion(record, failure) else {
            return;
        };
        self.publish_completions(std::slice::from_ref(&completion));
    }

    pub(crate) fn prepare_completion(
        &self,
        record: &SharedTaskRecord,
        failure: Option<TaskFailure>,
    ) -> Option<CompletionUpdate> {
        let mut task_record = record.lock();
        if task_record.status.is_terminal() {
            return None;
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
        let completion = CompletionUpdate {
            id: task_record.id,
            scope: task_record.scope,
            carrier: task_record.carrier,
            status: task_record.status,
        };
        #[cfg(feature = "runtime-evidence")]
        let terminal = (task_record.id, task_record.status, failure);
        drop(task_record);
        #[cfg(feature = "runtime-evidence")]
        self.record_terminal(terminal.0, terminal.1, terminal.2);
        record.completion().complete();
        Some(completion)
    }

    pub(crate) fn publish_completions(&self, completions: &[CompletionUpdate]) {
        if completions.is_empty() {
            return;
        }
        let mut state = lock(&self.state);
        let mut scope_drained = false;
        for completion in completions {
            if let Some(scope) = state.scopes.get_mut(&completion.scope) {
                scope.active -= 1;
                scope_drained |= scope.active == 0;
                scope.activity = scope.activity.wrapping_add(1);
                match completion.status {
                    TaskStatus::Aborted => {
                        scope.aborted += 1;
                        scope.failed_tasks.push(completion.id);
                    }
                    TaskStatus::Panicked => {
                        scope.panicked += 1;
                        scope.failed_tasks.push(completion.id);
                    }
                    _ => scope.completed += 1,
                }
            }
            state.active -= 1;
            state.loads[completion.carrier.0] -= 1;
        }
        let notify = scope_drained
            || state.active == 0
            || self.target_waiters.load(Ordering::SeqCst) != 0
            || self.config.stall_policy().timeout().is_some();
        drop(state);
        if notify {
            self.changed.notify();
        }
    }

    pub(crate) fn may_defer_completion(&self) -> bool {
        self.target_waiters.load(Ordering::SeqCst) == 0
            && self.config.stall_policy().timeout().is_none()
    }
}

#[cfg(test)]
#[path = "control_completion_test.rs"]
mod control_completion_test;
