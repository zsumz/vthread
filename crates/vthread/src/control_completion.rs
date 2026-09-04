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

pub(crate) struct CompletionBatch {
    first: Option<CompletionUpdate>,
    len: usize,
    completed: u64,
    panicked: u64,
    aborted: u64,
    failed_tasks: Vec<crate::TaskId>,
}

impl CompletionBatch {
    pub(crate) const fn new() -> Self {
        Self {
            first: None,
            len: 0,
            completed: 0,
            panicked: 0,
            aborted: 0,
            failed_tasks: Vec::new(),
        }
    }

    pub(crate) fn push(&mut self, completion: CompletionUpdate) {
        if let Some(first) = self.first {
            assert_eq!(completion.scope, first.scope, "completion batch scope");
            assert_eq!(
                completion.carrier, first.carrier,
                "completion batch carrier"
            );
        } else {
            self.first = Some(completion);
        }
        self.len += 1;
        match completion.status {
            TaskStatus::Completed => self.completed += 1,
            TaskStatus::Panicked => {
                self.panicked += 1;
                self.failed_tasks.push(completion.id);
            }
            TaskStatus::Aborted => {
                self.aborted += 1;
                self.failed_tasks.push(completion.id);
            }
            _ => panic!("completion batch contained a live task"),
        }
    }

    pub(crate) fn scope(&self) -> Option<u64> {
        self.first.map(|completion| completion.scope)
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(crate) fn clear(&mut self) {
        self.first = None;
        self.len = 0;
        self.completed = 0;
        self.panicked = 0;
        self.aborted = 0;
        self.failed_tasks.clear();
    }
}

impl Shared {
    pub(crate) fn complete(&self, record: &SharedTaskRecord, failure: Option<TaskFailure>) {
        let Some(completion) = self.prepare_completion(record, failure) else {
            return;
        };
        record.completion().complete();
        let mut batch = CompletionBatch::new();
        let progress = self.scope_progress(completion.scope);
        batch.push(completion);
        self.publish_completions(&batch, &progress);
    }

    pub(crate) fn prepare_completion(
        &self,
        record: &SharedTaskRecord,
        failure: Option<TaskFailure>,
    ) -> Option<CompletionUpdate> {
        #[cfg(feature = "lifecycle-profiling")]
        let completion_started = std::time::Instant::now();
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
        #[cfg(feature = "lifecycle-profiling")]
        self.lifecycle_probe
            .record_completion(completion_started.elapsed(), 0);
        Some(completion)
    }

    pub(crate) fn publish_completions(
        &self,
        completions: &CompletionBatch,
        progress: &super::ScopeProgress,
    ) {
        let Some(first) = completions.first else {
            return;
        };
        let stall_detection = self.config.stall_policy().timeout().is_some();
        #[cfg(feature = "lifecycle-profiling")]
        let completion_started = std::time::Instant::now();
        let state = stall_detection.then(|| lock(&self.state));
        let scope_drained = progress.retire(
            completions.len(),
            completions.completed,
            completions.panicked,
            completions.aborted,
            &completions.failed_tasks,
            stall_detection,
        );
        self.inboxes[first.carrier.0].retire_tasks(completions.len());
        let notify =
            scope_drained || self.target_waiters.load(Ordering::SeqCst) != 0 || stall_detection;
        drop(state);
        #[cfg(feature = "lifecycle-profiling")]
        self.lifecycle_probe
            .record_completion(completion_started.elapsed(), completions.len());
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
