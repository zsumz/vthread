//! Reconcile lexical stack revocation before inspecting generation timers or wakes.

use super::Kernel;
use crate::TaskFailure;

impl Kernel {
    pub(super) fn sweep_revoked(&mut self) {
        for _ in 0..self.ready.len() {
            let task = self.ready.pop_front().expect("ready task");
            if task.fiber.as_ref().is_some_and(|fiber| fiber.revoked()) {
                self.in_flight = Some(task);
                self.discard_in_flight(TaskFailure::ScopeClosed);
            } else {
                self.ready.push_back(task);
            }
        }
        let tokens = self
            .parked
            .iter()
            .filter(|(_, parked)| {
                parked
                    .task
                    .fiber
                    .as_ref()
                    .is_some_and(|fiber| fiber.revoked())
            })
            .map(|(token, _)| *token)
            .collect::<Vec<_>>();
        for token in tokens {
            let parked = self.parked.remove(&token).expect("revoked park");
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
            self.discard_in_flight(TaskFailure::ScopeClosed);
        }
    }
}

#[cfg(test)]
#[path = "kernel_revoked_test.rs"]
mod kernel_revoked_test;
