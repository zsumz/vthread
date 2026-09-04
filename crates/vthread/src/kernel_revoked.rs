//! Reconcile lexical stack revocation before inspecting generation timers or wakes.

use super::Kernel;
use crate::TaskFailure;

impl Kernel {
    pub(super) fn sweep_revoked(&mut self) {
        if !self.has_borrowed {
            return;
        }
        for _ in 0..self.ready.len() {
            let task = self.ready.pop_front().expect("ready task");
            #[cfg(test)]
            {
                self.revocation_inspections += 1;
            }
            if self.task(task).revoked() {
                self.in_flight = Some(task);
                self.discard_in_flight(TaskFailure::ScopeClosed);
            } else {
                self.ready.push_back(task);
            }
        }
        let tasks = self
            .parked
            .iter()
            .filter(|parked| self.task(parked.task).revoked())
            .map(|parked| parked.task)
            .collect::<Vec<_>>();
        for task in tasks {
            let parked = self.parked.remove(task).expect("revoked park");
            let token = parked.token;
            self.local.unregister_wake(token);
            parked.registration.abandon(token);
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
            self.discard_in_flight(TaskFailure::ScopeClosed);
        }
        self.refresh_borrowed();
    }

    pub(super) fn refresh_borrowed(&mut self) {
        self.has_borrowed = self
            .in_flight
            .is_some_and(|task| self.task(task).is_borrowed())
            || self.ready.iter().any(|task| self.task(*task).is_borrowed())
            || self
                .parked
                .iter()
                .any(|parked| self.task(parked.task).is_borrowed());
    }
}

#[cfg(test)]
#[path = "kernel_revoked_test.rs"]
mod kernel_revoked_test;
