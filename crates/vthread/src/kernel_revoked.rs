//! Reconcile lexical stack revocation before inspecting generation timers or wakes.

use super::Kernel;
use crate::TaskFailure;

impl Kernel {
    pub(super) fn sweep_revoked(&mut self) {
        if !self.has_borrowed {
            return;
        }
        let epoch = self.local.borrowed_scope_epoch();
        if epoch == self.observed_borrowed_scope_epoch {
            return;
        }
        self.observed_borrowed_scope_epoch = epoch;
        let mut inspection = self.ready.inspection();
        while let Some(task) = self.ready.remove_matching(&mut inspection, |task| {
            #[cfg(test)]
            {
                self.revocation_inspections += 1;
            }
            self.tasks.get(task).expect("ready task").revoked()
        }) {
            self.in_flight = Some(task);
            self.discard_in_flight(TaskFailure::ScopeClosed);
        }
        let tasks = self
            .parked
            .iter()
            .filter(|parked| self.task(parked.task).revoked())
            .map(|parked| parked.task)
            .collect::<Vec<_>>();
        for task in tasks {
            let parked = self.remove_parked(task);
            let token = parked.token;
            self.local.unregister_wake(token);
            if let Some(registration) = parked.registration {
                assert!(
                    registration.try_abandon(token),
                    "revoked frame retained a publisher"
                );
            } else {
                assert!(
                    self.task(parked.task)
                        .execution()
                        .try_abandon_synchronization_wait(token),
                    "revoked frame retained a publisher"
                );
            }
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
        self.has_borrowed = self.tasks.borrowed_count() != 0;
    }
}

#[cfg(test)]
#[path = "kernel_revoked_test.rs"]
mod kernel_revoked_test;

#[cfg(test)]
#[path = "kernel_revoked_queue_test.rs"]
mod kernel_revoked_queue_test;
