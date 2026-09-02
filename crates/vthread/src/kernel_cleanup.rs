//! Reclaim every carrier-owned stack before publishing task failure.

use super::Kernel;
use crate::{CarrierStatus, TaskFailure, context};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

impl Kernel {
    pub(crate) fn abort(&mut self, scope: Option<u64>, reason: TaskFailure) {
        if self
            .pending
            .as_ref()
            .is_some_and(|packet| scope.is_none_or(|scope| packet.record.lock().scope == scope))
        {
            self.discard_pending(reason);
        }
        let retained_pending = self.pending.take();
        while let Some(packet) = self.inbox.pop_scope(scope) {
            self.pending = Some(packet);
            self.discard_pending(reason);
            #[cfg(test)]
            assert_ne!(
                self.shared
                    .carrier_fault
                    .load(std::sync::atomic::Ordering::Acquire),
                1,
                "injected carrier failure during partial inbox abort"
            );
        }
        self.pending = retained_pending;
        if self.in_flight.is_some_and(|task| {
            scope.is_none_or(|scope| self.task(task).execution.record.lock().scope == scope)
        }) {
            self.discard_in_flight(reason);
        }
        let retained_flight = self.in_flight.take();
        if let Some(task) = retained_flight {
            self.ready.push_front(task);
        }
        for task in self.local.take_starts() {
            self.ready.push_back(self.tasks.insert(task));
        }
        for _ in 0..self.ready.len() {
            let task = self.ready.pop_front().expect("ready task");
            if scope.is_none_or(|scope| self.task(task).execution.record.lock().scope == scope) {
                self.in_flight = Some(task);
                self.discard_in_flight(reason);
            } else {
                self.ready.push_back(task);
            }
        }
        let tokens = self
            .parked
            .iter()
            .filter(|(_, parked)| {
                scope.is_none_or(|scope| {
                    self.task(parked.task).execution.record.lock().scope == scope
                })
            })
            .map(|(token, _)| *token)
            .collect::<Vec<_>>();
        for token in tokens {
            let parked = self.parked.remove(&token).expect("owned park");
            parked.registration.abandon(token);
            self.inbox.hub.unregister(token);
            if self.timers.cancel(token) {
                #[cfg(feature = "runtime-evidence")]
                self.shared.record(
                    crate::diagnostics::evidence::RuntimeEventKind::TimerRetired {
                        wait: crate::diagnostics::evidence::WaitKey::from_token(token),
                        carrier: self.id,
                        reason: crate::diagnostics::evidence::TimerRetirement::TaskReclaimed,
                    },
                );
            }
            self.in_flight = Some(parked.task);
            self.discard_in_flight(reason);
        }
        if let Some(retained) = retained_flight {
            let restored = self.ready.pop_front().expect("retained in-flight task");
            assert_eq!(restored, retained, "retained in-flight task moved");
            self.in_flight = Some(restored);
        }
        self.refresh_borrowed();
        self.publish(CarrierStatus::Running);
    }

    pub(super) fn discard_pending(&mut self, reason: TaskFailure) {
        let packet = self.pending.as_mut().expect("pending packet");
        let record = Arc::clone(&packet.record);
        let entry = packet.entry.take();
        {
            let _mounted = context::mount(record.lock().id, Arc::clone(&self.inbox.hub));
            if let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(entry))) {
                let panic = crate::PanicReport::capture(payload);
                record.lock().panic.get_or_insert(panic);
            }
        }
        self.pending = None;
        self.stats.aborted += 1;
        self.publish(CarrierStatus::Running);
        self.shared.complete(&record, Some(reason));
    }

    pub(super) fn discard_in_flight(&mut self, reason: TaskFailure) {
        let task_key = self.in_flight.expect("owned task key");
        let execution = self.execution(task_key);
        execution.progress.unmount(execution.record.progress());
        execution.data.closing.set(true);
        let task = self.task_mut(task_key);
        let record = Arc::clone(&task.execution.record);
        let fiber = task.fiber.take();
        #[cfg(feature = "runtime-evidence")]
        let stack = fiber
            .as_ref()
            .map(crate::task_fiber::TaskFiber::stack_identity);
        {
            // Destructors still belong to this task and must not block its carrier
            // through scope entry, joins, or explicit runtime shutdown.
            let _cleanup = crate::task_context::TaskCleanup::new(execution);
            if let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(fiber))) {
                let panic = crate::PanicReport::capture(payload);
                record.lock().panic.get_or_insert(panic);
            }
        }
        #[cfg(feature = "runtime-evidence")]
        if let Some(identity) = stack {
            self.local.stacks.borrow_mut().retire(identity);
            self.shared.record(
                crate::diagnostics::evidence::RuntimeEventKind::StackReleased {
                    task: record.lock().id,
                    stack: crate::diagnostics::evidence::StackId::new(self.id, identity),
                    disposition: crate::diagnostics::evidence::StackDisposition::Discarded,
                },
            );
        }
        drop(self.remove_in_flight());
        self.stats.aborted += 1;
        self.publish(CarrierStatus::Running);
        self.shared.complete(&record, Some(reason));
    }
}

#[cfg(test)]
#[path = "kernel_cleanup_test.rs"]
mod kernel_cleanup_test;
