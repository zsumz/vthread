//! Cleanup and reset of carrier-cached task contexts.

use super::{TaskContext, TaskPolicy};
use crate::{SuspensionReason, context, options::TaskOptions};
use std::rc::Rc;

pub(crate) struct TaskCleanup {
    execution: Rc<context::Execution>,
    _mounted: context::MountGuard,
}

impl TaskCleanup {
    pub(crate) fn new(execution: Rc<context::Execution>) -> Self {
        execution.data.close();
        Self::mount(execution)
    }

    pub(crate) fn completed(execution: &Rc<context::Execution>) -> Option<Self> {
        execution.data.close();
        if execution.data.cold.values.borrow().is_empty() {
            return None;
        }
        Some(Self::mount(Rc::clone(execution)))
    }

    fn mount(execution: Rc<context::Execution>) -> Self {
        let mounted = context::mount_execution(Rc::clone(&execution));
        Self {
            execution,
            _mounted: mounted,
        }
    }
}

impl Drop for TaskCleanup {
    fn drop(&mut self) {
        if let Some(panic) = self.execution.data.clear() {
            self.execution.record().lock().panic.get_or_insert(panic);
        }
    }
}

impl TaskContext {
    pub(crate) fn recycle(&mut self, cancellation: crate::CancellationToken) {
        assert!(
            self.cold.values.get_mut().is_empty(),
            "recycled task locals"
        );
        self.policy = TaskPolicy::new(cancellation, false);
    }

    pub(crate) fn reuse(&mut self, options: TaskOptions, capacity: usize) {
        let deadline = options.deadline;
        self.policy = TaskPolicy::new(options.cancellation, deadline.is_some());
        self.cold.deadline = deadline;
        self.cold.reason.set(SuspensionReason::Park);
        self.cold.capacity = capacity;
        assert!(self.cold.values.get_mut().is_empty(), "reused task locals");
    }
}

#[cfg(test)]
#[path = "task_context_reuse_test.rs"]
mod task_context_reuse_test;
