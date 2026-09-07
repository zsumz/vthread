//! Bounded scope-level retention before any ancestor can reclaim borrowed frames.

use super::Kernel;
use crate::TaskFailure;

impl Kernel {
    pub(super) fn prepare_abort(&self, scope: Option<u64>) -> bool {
        let mut retired = true;
        for parked in self.parked.iter().filter(|parked| {
            scope.is_none_or(|scope| self.task(parked.task).execution().scope() == scope)
        }) {
            // An Active observation is insufficient: a selector could claim
            // after it. Retire atomically, or register completion interest and
            // keep every ancestor in this root alive until a later attempt.
            let complete = if let Some(registration) = &parked.registration {
                registration.try_abandon(parked.token)
            } else {
                self.task(parked.task)
                    .execution()
                    .try_abandon_synchronization_wait(parked.token)
            };
            retired &= complete;
        }
        retired
    }

    pub(super) fn defer_abort(&mut self, scope: Option<u64>, reason: TaskFailure) {
        if scope.is_none() {
            self.pending_aborts.clear();
        } else if self.pending_aborts.iter().any(|(scope, _)| scope.is_none()) {
            return;
        }
        if let Some((_, pending)) = self
            .pending_aborts
            .iter_mut()
            .find(|(old, _)| *old == scope)
        {
            *pending = reason;
        } else {
            assert!(
                self.pending_aborts.len() < self.shared.config.max_owned_scopes(),
                "deferred cleanup exceeds bounded scope admission"
            );
            self.pending_aborts.push((scope, reason));
        }
        if self.in_flight.is_some_and(|task| {
            scope.is_none_or(|scope| self.task(task).execution().scope() == scope)
        }) {
            self.ready
                .push_front(self.in_flight.take().expect("retained ancestor"));
        }
    }

    pub(super) fn retry_aborts(&mut self) {
        for index in (0..self.pending_aborts.len()).rev() {
            let (scope, reason) = self.pending_aborts.swap_remove(index);
            self.abort(scope, reason);
        }
    }

    #[cold]
    pub(super) fn select_unblocked(&mut self) -> Option<crate::task_slab::TaskKey> {
        let tasks = &self.tasks;
        let pending = &self.pending_aborts;
        self.ready.pop_matching(|task| {
            let task = tasks.get(task).expect("ready task");
            !pending
                .iter()
                .any(|(scope, _)| scope.is_none_or(|scope| task.execution().scope() == scope))
        })
    }
}

#[cfg(test)]
#[path = "kernel_abort_test.rs"]
mod kernel_abort_test;
