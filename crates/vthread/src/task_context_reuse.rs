//! Resetting carrier-cached task context without changing its hot layout.

use super::{TaskContext, TaskPolicy};
use crate::{SuspensionReason, options::TaskOptions};

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
