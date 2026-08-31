//! Reclaim every carrier-owned stack before publishing task failure.

use super::Kernel;
use crate::{CarrierStatus, TaskFailure, context, signal::lock};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

impl Kernel {
    pub(crate) fn abort(&mut self, scope: Option<u64>, reason: TaskFailure) {
        if self
            .pending
            .as_ref()
            .is_some_and(|packet| scope.is_none_or(|scope| lock(&packet.record).scope == scope))
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
        if self
            .in_flight
            .as_ref()
            .is_some_and(|task| scope.is_none_or(|scope| lock(&task.record).scope == scope))
        {
            self.discard_in_flight(reason);
        }
        let retained_flight = self.in_flight.take();
        let local = self.local.starts.take();
        self.ready.extend(local);
        for _ in 0..self.ready.len() {
            let task = self.ready.pop_front().expect("ready task");
            if scope.is_none_or(|scope| lock(&task.record).scope == scope) {
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
                scope.is_none_or(|scope| lock(&parked.task.record).scope == scope)
            })
            .map(|(token, _)| *token)
            .collect::<Vec<_>>();
        for token in tokens {
            let parked = self.parked.remove(&token).expect("owned park");
            parked.registration.abandon(token);
            self.inbox.hub.unregister(token);
            self.timers.cancel(token);
            self.in_flight = Some(parked.task);
            self.discard_in_flight(reason);
        }
        self.in_flight = retained_flight;
        self.publish(CarrierStatus::Running);
    }

    pub(super) fn discard_pending(&mut self, reason: TaskFailure) {
        let packet = self.pending.as_mut().expect("pending packet");
        let record = Arc::clone(&packet.record);
        let entry = packet.entry.take();
        {
            let _mounted = context::mount(lock(&record).id, Arc::clone(&self.inbox.hub));
            if let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(entry))) {
                let panic = crate::PanicReport::capture(payload);
                lock(&record).panic.get_or_insert(panic);
            }
        }
        self.pending = None;
        self.stats.aborted += 1;
        self.publish(CarrierStatus::Running);
        self.shared.complete(&record, Some(reason));
    }

    pub(super) fn discard_in_flight(&mut self, reason: TaskFailure) {
        let execution = self.execution(self.in_flight.as_ref().expect("owned task"));
        execution.data.closing.set(true);
        let task = self.in_flight.as_mut().expect("owned task");
        let record = Arc::clone(&task.record);
        let fiber = task.fiber.take();
        {
            // Destructors still belong to this task and must not block its carrier
            // through scope entry, joins, or explicit runtime shutdown.
            let _cleanup =
                crate::task_context::TaskCleanup::new(execution, Arc::clone(&self.inbox.hub));
            if let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(fiber))) {
                let panic = crate::PanicReport::capture(payload);
                lock(&record).panic.get_or_insert(panic);
            }
        }
        self.in_flight = None;
        self.stats.aborted += 1;
        self.publish(CarrierStatus::Running);
        self.shared.complete(&record, Some(reason));
    }
}

#[cfg(test)]
#[path = "kernel_cleanup_test.rs"]
mod kernel_cleanup_test;
