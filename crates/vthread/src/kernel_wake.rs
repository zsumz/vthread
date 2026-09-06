//! Validate publication before mounting; incomplete generations remain owner-held.

use super::{Kernel, ParkedTask};
use crate::{
    Error, Result, TaskStatus, WakeReason,
    wait::{Publication, WakeCause, WakeNotice},
};

impl Kernel {
    pub(super) fn process_wakes(&mut self) -> Result<()> {
        let mut processed = false;
        while let Some(notice) = self.local.pop_wake().or_else(|| self.inbox.hub.pop_wake()) {
            processed = true;
            self.process_wake(notice)?;
        }
        if processed {
            self.publish_transition();
        }
        Ok(())
    }

    pub(super) fn process_deferred_wakes(&mut self) -> Result<()> {
        // Only changed completion/control epochs revisit these records. A held
        // publisher cannot force an ordinary dispatch to scan parked capacity.
        for index in (0..self.deferred_wakes.len()).rev() {
            let notice = self.deferred_wakes.swap_remove(index);
            let Some(parked) = self.parked.get_mut(notice.route) else {
                self.stats.stale_wakes += 1;
                continue;
            };
            assert_eq!(
                parked.token, notice.token,
                "deferred route reused before retirement"
            );
            parked.deferred = false;
            self.process_wake(notice)?;
        }
        Ok(())
    }

    fn process_wake(&mut self, notice: WakeNotice) -> Result<()> {
        let Some(parked) = self.parked.get(notice.route) else {
            self.stats.stale_wakes += 1;
            return Ok(());
        };
        if self.task(parked.task).execution().id != notice.task {
            return Err(Error::fault(
                crate::error::FaultComponent::Scheduler,
                "wake notice task does not own wait token",
            ));
        }
        if parked.token != notice.token {
            self.stats.stale_wakes += 1;
            return Ok(());
        }
        let publication = if let Some(registration) = &parked.registration {
            registration.publication(notice.token)
        } else {
            self.task(parked.task)
                .execution()
                .synchronization_publication(notice.token)
        };
        match publication {
            Publication::Published => self.finish_wake(notice),
            Publication::InFlight => {
                let parked = self
                    .parked
                    .get_mut(notice.route)
                    .expect("validated park route");
                if !parked.deferred {
                    parked.deferred = true;
                    assert!(
                        self.deferred_wakes.len() < self.shared.config.max_vthreads(),
                        "deferred wakes exceed live-task admission"
                    );
                    self.deferred_wakes.push(notice);
                }
            }
            Publication::Stale => self.stats.stale_wakes += 1,
        }
        Ok(())
    }

    pub(super) fn remove_parked(&mut self, task: crate::task_slab::TaskKey) -> ParkedTask {
        let parked = self.parked.remove(task).expect("owned park route");
        if parked.deferred {
            let index = self
                .deferred_wakes
                .iter()
                .position(|notice| notice.route == task && notice.token == parked.token)
                .expect("owned deferred generation");
            self.deferred_wakes.swap_remove(index);
        }
        parked
    }

    fn finish_wake(&mut self, notice: WakeNotice) {
        let parked = self.remove_parked(notice.route);
        if parked.has_deadline && self.timers.cancel(notice.token) {
            #[cfg(feature = "runtime-evidence")]
            self.shared.record(
                crate::diagnostics::evidence::RuntimeEventKind::TimerRetired {
                    wait: crate::diagnostics::evidence::WaitKey::from_token(notice.token),
                    carrier: self.id,
                    reason: crate::diagnostics::evidence::TimerRetirement::WakeSelected,
                },
            );
        }
        let wake = match notice.cause {
            WakeCause::Ready => WakeReason::Ready,
            WakeCause::TimedOut => WakeReason::TimedOut,
            WakeCause::Cancelled | WakeCause::InheritedCancelled => WakeReason::Cancelled,
            WakeCause::Closed => WakeReason::Closed,
        };
        self.task(parked.task)
            .execution()
            .record()
            .progress()
            .wake(wake);
        if parked.has_deadline || self.shared.config.stall_policy().timeout().is_some() {
            self.shared
                .transition(self.task(parked.task).execution().record(), |record| {
                    record.status = TaskStatus::Ready;
                    record.deadline = None;
                    record.last_wake = Some(wake);
                });
        }
        self.stats.wakes += 1;
        match notice.cause {
            WakeCause::Ready => {}
            WakeCause::TimedOut => self.stats.timeouts += 1,
            WakeCause::Cancelled | WakeCause::InheritedCancelled => self.stats.cancelled += 1,
            WakeCause::Closed => self.stats.closed += 1,
        }
        self.ready.push_wake(parked.task);
    }
}

#[cfg(test)]
#[path = "kernel_wake_test.rs"]
mod kernel_wake_test;
