//! Completed-stack reclamation and terminal publication.

use super::Kernel;
use crate::CarrierStatus;
use std::sync::Arc;

impl Kernel {
    pub(super) fn complete_task(&mut self) {
        self.yield_pressure = 0;
        let task_key = self.in_flight.expect("completed task key");
        let record = Arc::clone(self.task(task_key).execution().record());
        #[cfg(feature = "lifecycle-profiling")]
        let reclaim_started = std::time::Instant::now();
        #[cfg(feature = "runtime-evidence")]
        let task = record.lock().id;
        {
            let _cleanup =
                crate::task_context::TaskCleanup::completed(self.task(task_key).execution());
            #[cfg(feature = "runtime-evidence")]
            let (identity, retained) = self
                .tasks
                .get_mut(task_key)
                .expect("completed task")
                .take_fiber()
                .expect("completed stack")
                .reclaim_stack(&mut self.local.stacks.borrow_mut());
            #[cfg(not(feature = "runtime-evidence"))]
            self.tasks
                .get_mut(task_key)
                .expect("completed task")
                .take_fiber()
                .expect("completed stack")
                .reclaim_stack(&mut self.local.stacks.borrow_mut());
            #[cfg(feature = "runtime-evidence")]
            self.shared.record(
                crate::diagnostics::evidence::RuntimeEventKind::StackReleased {
                    task,
                    stack: crate::diagnostics::evidence::StackId::new(self.id, identity),
                    disposition: if retained {
                        crate::diagnostics::evidence::StackDisposition::Cached
                    } else {
                        crate::diagnostics::evidence::StackDisposition::Discarded
                    },
                },
            );
        }
        self.recycle_execution(task_key);
        self.remove_in_flight();
        #[cfg(feature = "lifecycle-profiling")]
        self.shared
            .lifecycle_probe
            .record_reclaim(reclaim_started.elapsed());
        let completion = self
            .shared
            .prepare_completion(&record, None)
            .expect("live completed task");
        if completion.status == crate::TaskStatus::Panicked {
            self.stats.panicked += 1;
        } else {
            self.stats.completed += 1;
        }
        if self.ready.is_empty() {
            self.publish(CarrierStatus::Running);
        } else {
            self.publish_transition();
        }
        // Completion and admission release become visible only after reclaiming the stack.
        self.queue_completion(completion);
        if self.ready.is_empty() {
            self.flush_completions();
        }
    }
}

#[cfg(test)]
#[path = "kernel_complete_test.rs"]
mod kernel_complete_test;
