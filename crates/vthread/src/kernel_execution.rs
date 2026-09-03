//! Bounded reuse of carrier-affine execution and task-context allocations.

use super::Kernel;
use crate::{TaskId, context::Execution, options::TaskOptions, task::SharedTaskRecord};
use std::{rc::Rc, sync::Arc};

impl Kernel {
    pub(super) fn acquire_execution(
        &mut self,
        id: TaskId,
        scope: u64,
        record: SharedTaskRecord,
        options: TaskOptions,
    ) -> Rc<Execution> {
        let Some(mut execution) = self.execution_cache.pop() else {
            let data = Rc::new(crate::task_context::TaskContext::new(
                options,
                self.shared.config.task_local_capacity(),
            ));
            return Rc::new(Execution::new(
                id,
                scope,
                Arc::clone(&self.inbox.hub),
                record,
                Arc::clone(&self.shared),
                Rc::clone(&self.local),
                data,
            ));
        };
        Rc::get_mut(&mut execution)
            .expect("cached execution must be unique")
            .reuse(
                id,
                scope,
                record,
                options,
                self.shared.config.task_local_capacity(),
            );
        execution
    }

    pub(super) fn recycle_execution(&mut self, task: crate::task_slab::TaskKey) {
        let mut execution = self
            .task_mut(task)
            .take_execution()
            .expect("live task execution");
        if self.execution_cache.len() == self.shared.config.stack_cache_capacity() {
            return;
        }
        let Some(execution_mut) = Rc::get_mut(&mut execution) else {
            return;
        };
        if !execution_mut.recycle() {
            return;
        }
        self.execution_cache.push(execution);
    }
}

#[cfg(test)]
#[path = "kernel_execution_test.rs"]
mod kernel_execution_test;
