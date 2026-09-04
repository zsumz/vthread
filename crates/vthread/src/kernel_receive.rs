//! Fair bounded materialization of remote starts and unbounded local borrows.

use super::Kernel;
use crate::{CarrierStatus, TaskFailure, TaskStatus, kernel_tasks::OwnedTask};
use std::sync::Arc;
use vthread_stack::Fiber;

const REMOTE_READY_TARGET: usize = 64;
const REMOTE_ADMISSION_YIELD_BOUND: u32 = 65_536;
// Bridge short multi-carrier admission gaps without turning an idle carrier into a poller.
// The carrier performs at most 640 pause instructions before entering the signal wait.
const IDLE_SIGNAL_PROBES: usize = 640;
const SPINS_PER_SIGNAL_PROBE: usize = 1;

impl Kernel {
    pub(crate) fn receive(&mut self) -> bool {
        let received = self.receive_local_tasks();
        let received = self.receive_remote_tasks() || received;
        if received {
            self.publish(CarrierStatus::Running);
        }
        self.remote_pending = self.inbox.pending() != 0;
        if !self.remote_pending {
            self.yield_pressure = 0;
        }
        self.remote_pending
    }

    pub(crate) fn remote_pending(&self) -> bool {
        self.remote_pending
    }

    pub(crate) fn receive_local(&mut self) {
        if self.receive_local_tasks() {
            self.publish(CarrierStatus::Running);
        }
    }

    fn receive_local_tasks(&mut self) -> bool {
        let mut received = false;
        while let Some(task) = self.local.pop_start() {
            received = true;
            self.has_borrowed = true;
            #[cfg(feature = "runtime-evidence")]
            self.shared
                .record(crate::diagnostics::evidence::RuntimeEventKind::QueueDepth {
                    carrier: self.id,
                    queue: crate::diagnostics::evidence::QueueKind::LocalStart,
                    depth: self.local.pending_starts(),
                    capacity: self.shared.config.carrier_queue_capacity(),
                });
            self.shared.transition(
                task.execution.as_ref().expect("task execution").record(),
                |record| record.status = TaskStatus::Ready,
            );
            self.ready.push_back(self.tasks.insert_borrowed(task));
        }
        received
    }

    fn receive_remote_tasks(&mut self) -> bool {
        assert!(self.incoming.is_empty(), "unprocessed start batch");
        let capacity = self.shared.config.carrier_queue_capacity();
        let target = REMOTE_READY_TARGET.min(capacity);
        let limit = if self.ready.len() <= target / 2 {
            target - self.ready.len()
        // Keep completion-heavy work inside the hot window, but a carrier whose
        // window only yields must still admit later tasks within a fixed bound.
        } else if self.yield_pressure >= REMOTE_ADMISSION_YIELD_BOUND && self.inbox.pending() != 0 {
            capacity
        } else {
            return false;
        };
        self.yield_pressure = 0;
        self.inbox.drain_into(&mut self.incoming, limit);
        let mut received = false;
        while let Some(packet) = self.incoming.pop_front() {
            self.pending = Some(packet);
            #[cfg(feature = "lifecycle-profiling")]
            let stack_fiber_started = std::time::Instant::now();
            #[cfg(feature = "runtime-evidence")]
            let acquired = self.local.stacks.borrow_mut().acquire_identified();
            #[cfg(not(feature = "runtime-evidence"))]
            let acquired = self.local.stacks.borrow_mut().acquire();
            #[cfg(feature = "runtime-evidence")]
            let (stack_identity, stack) = match acquired {
                Ok(stack) => stack,
                Err(_) => {
                    self.discard_pending(TaskFailure::StackAllocation);
                    #[cfg(feature = "lifecycle-profiling")]
                    self.shared
                        .lifecycle_probe
                        .record_stack_fiber(stack_fiber_started.elapsed());
                    continue;
                }
            };
            #[cfg(not(feature = "runtime-evidence"))]
            let stack = match acquired {
                Ok(stack) => stack,
                Err(_) => {
                    self.discard_pending(TaskFailure::StackAllocation);
                    #[cfg(feature = "lifecycle-profiling")]
                    self.shared
                        .lifecycle_probe
                        .record_stack_fiber(stack_fiber_started.elapsed());
                    continue;
                }
            };
            let packet = self.pending.as_mut().expect("pending packet");
            received = true;
            let entry = packet.entry.take().expect("unstarted packet entry");
            let body_record = Arc::clone(&packet.record);
            let fiber = Fiber::new(stack, move || entry.run(&body_record));
            #[cfg(feature = "runtime-evidence")]
            let task_fiber = crate::task_fiber::OwnedFiber::new(fiber, stack_identity);
            #[cfg(not(feature = "runtime-evidence"))]
            let task_fiber = crate::task_fiber::OwnedFiber::new(fiber);
            let (id, scope, options) = self.shared.transition(&packet.record, |record| {
                record.status = TaskStatus::Ready;
                (record.id, record.scope, record.options().clone())
            });
            let record = Arc::clone(&packet.record);
            let execution = self.acquire_execution(id, scope, record, options);
            let task = OwnedTask {
                fiber: Some(task_fiber),
                execution: Some(execution),
            };
            #[cfg(feature = "runtime-evidence")]
            self.shared.record(
                crate::diagnostics::evidence::RuntimeEventKind::StackCheckedOut {
                    task: id,
                    stack: crate::diagnostics::evidence::StackId::new(self.id, stack_identity),
                },
            );
            self.pending = None;
            self.ready.push_back(self.tasks.insert_owned(task));
            #[cfg(feature = "lifecycle-profiling")]
            self.shared
                .lifecycle_probe
                .record_stack_fiber(stack_fiber_started.elapsed());
        }
        received
    }

    pub(crate) fn wait_for_work(&mut self, observed: u64) {
        self.flush_completions();
        let deadline = self.timers.next_deadline();
        if deadline.is_some() {
            self.stats.timer_sleeps += 1;
        }
        if self.inbox.pending() != 0
            || self.local.pending_wakes() != 0
            || self.inbox.hub.has_pending()
        {
            return;
        }
        if deadline.is_none() && self.shared.config.carriers() > 1 {
            for _ in 0..IDLE_SIGNAL_PROBES {
                for _ in 0..SPINS_PER_SIGNAL_PROBE {
                    std::hint::spin_loop();
                }
                if self.inbox.pending() != 0
                    || self.local.pending_wakes() != 0
                    || self.inbox.hub.has_pending()
                    || self.inbox.signal.version() != observed
                {
                    return;
                }
            }
        }
        self.publish(CarrierStatus::Idle);
        self.inbox.hub.wait(observed, deadline);
    }
}

#[cfg(test)]
#[path = "kernel_receive_test.rs"]
mod kernel_receive_test;
