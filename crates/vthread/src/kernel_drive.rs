//! Owner-only mounting, generation-checked waking, and timer progression.

use super::{Kernel, ParkedTask};
use crate::{
    CarrierStatus, Error, Result, SuspensionReason, TaskStatus, WakeReason, wait::WakeCause,
};
use std::{sync::Arc, time::Instant};
use vthread_stack::{FiberState, ParkRequest, Suspension};

impl Kernel {
    pub(crate) fn tick(&mut self, signal_changed: bool) -> Result<bool> {
        self.sweep_revoked();
        if signal_changed {
            self.process_wakes()?;
        }
        if self.timers.active_count() != 0 {
            for token in self.timers.pop_expired(Instant::now()) {
                #[cfg(feature = "runtime-evidence")]
                self.shared.record(
                    crate::diagnostics::evidence::RuntimeEventKind::TimerRetired {
                        wait: crate::diagnostics::evidence::WaitKey::from_token(token),
                        carrier: self.id,
                        reason: crate::diagnostics::evidence::TimerRetirement::Expired,
                    },
                );
                if let Some(parked) = self.parked.get(&token) {
                    parked.registration.select_timeout(token)?;
                }
            }
            self.process_wakes()?;
        }
        self.in_flight = self.ready.pop_front();
        let Some(task_key) = self.in_flight else {
            self.flush_completions();
            return Ok(false);
        };
        let scope = self.task(task_key).execution().scope();
        if self
            .completions
            .first()
            .is_some_and(|completion| completion.scope != scope)
        {
            self.flush_completions();
        }
        if self.shared.abort_requested() {
            let scope = self.task(task_key).execution().scope();
            if let Some(reason) = self.shared.abort_reason(scope) {
                self.discard_in_flight(reason);
                return Ok(true);
            }
        }
        self.stats.mounts += 1;
        let shared = &self.shared;
        let carrier_progress = &shared.carrier_progress[self.id.0];
        let dispatched = {
            let mut task = self.tasks.get_mut(task_key).expect("ready task key");
            let execution = task.execution();
            let first_mount = execution.progress.mount(carrier_progress, execution.id);
            if shared.config.stall_policy().timeout().is_some() {
                shared.transition(execution.record(), |record| {
                    record.status = TaskStatus::Running;
                });
            }
            #[cfg(feature = "runtime-evidence")]
            shared.record(crate::diagnostics::evidence::RuntimeEventKind::Mounted {
                task: execution.id,
                carrier: self.id,
            });
            (!first_mount).then(|| task.dispatch(shared, carrier_progress))
        };
        let state = if let Some(state) = dispatched {
            state
        } else {
            if self.ready.is_empty() {
                // A lone first mount may remain in user code indefinitely, so
                // there may be no later transition at which to flush it.
                self.publish(CarrierStatus::Running);
            } else {
                self.publish_transition();
            }
            let mut task = self.tasks.get_mut(task_key).expect("mounted task key");
            task.dispatch(shared, carrier_progress)
        };
        let publish = match state {
            Some(FiberState::Suspended(Suspension::YieldNow)) => {
                let task_key = self.in_flight.take().expect("yielded task key");
                #[cfg(feature = "runtime-evidence")]
                let task_id = self.task(task_key).execution().id;
                if self.shared.config.stall_policy().timeout().is_some() {
                    self.shared
                        .transition(self.task(task_key).execution().record(), |record| {
                            record.status = TaskStatus::Ready;
                            record.last_suspension = Some(SuspensionReason::YieldNow);
                        });
                }
                self.stats.yields += 1;
                self.yield_pressure = self.yield_pressure.saturating_add(1);
                #[cfg(feature = "runtime-evidence")]
                self.shared
                    .record(crate::diagnostics::evidence::RuntimeEventKind::Yielded {
                        task: task_id,
                        carrier: self.id,
                    });
                self.ready.push_back(task_key);
                false
            }
            Some(FiberState::Suspended(Suspension::Park(request))) => {
                self.park_task(request)?;
                true
            }
            Some(FiberState::Complete) => {
                self.complete_task();
                false
            }
            None => {
                self.discard_in_flight(crate::TaskFailure::ScopeClosed);
                false
            }
        };
        if publish {
            self.publish(CarrierStatus::Running);
        }
        Ok(true)
    }

    fn process_wakes(&mut self) -> Result<()> {
        let mut processed = false;
        while let Some(notice) = self.inbox.hub.pop_wake() {
            processed = true;
            let Some(parked) = self.parked.get(&notice.token) else {
                self.stats.stale_wakes += 1;
                continue;
            };
            if self.task(parked.task).execution().id != notice.task {
                return Err(Error::fault(
                    crate::error::FaultComponent::Scheduler,
                    "wake notice task does not own wait token",
                ));
            }
            let parked = self.parked.remove(&notice.token).expect("validated park");
            if self.timers.cancel(notice.token) {
                #[cfg(feature = "runtime-evidence")]
                self.shared.record(
                    crate::diagnostics::evidence::RuntimeEventKind::TimerRetired {
                        wait: crate::diagnostics::evidence::WaitKey::from_token(notice.token),
                        carrier: self.id,
                        reason: crate::diagnostics::evidence::TimerRetirement::WakeSelected,
                    },
                );
            }
            self.shared
                .transition(self.task(parked.task).execution().record(), |record| {
                    record.status = TaskStatus::Ready;
                    record.deadline = None;
                    record.last_wake = Some(match notice.cause {
                        WakeCause::Ready => WakeReason::Ready,
                        WakeCause::TimedOut => WakeReason::TimedOut,
                        WakeCause::Cancelled | WakeCause::InheritedCancelled => {
                            WakeReason::Cancelled
                        }
                        WakeCause::Closed => WakeReason::Closed,
                    });
                });
            self.stats.wakes += 1;
            match notice.cause {
                WakeCause::Ready => {}
                WakeCause::TimedOut => self.stats.timeouts += 1,
                WakeCause::Cancelled | WakeCause::InheritedCancelled => self.stats.cancelled += 1,
                WakeCause::Closed => self.stats.closed += 1,
            }
            self.ready.push_back(parked.task);
        }
        if processed {
            self.publish(CarrierStatus::Running);
        }
        Ok(())
    }

    fn park_task(&mut self, request: ParkRequest) -> Result<()> {
        self.yield_pressure = 0;
        let token = request.token();
        if self.parked.contains_key(&token) {
            return Err(Error::fault(
                crate::error::FaultComponent::Scheduler,
                "wait token parked twice",
            ));
        }
        let registration = self.inbox.hub.take_registration(token)?;
        if let Some(deadline) = request.deadline()
            && !self.timers.schedule(token, deadline)
        {
            return Err(Error::fault(
                crate::error::FaultComponent::Scheduler,
                "wait timer scheduled twice",
            ));
        }
        #[cfg(feature = "runtime-evidence")]
        if request.deadline().is_some() {
            self.shared.record(
                crate::diagnostics::evidence::RuntimeEventKind::TimerRegistered {
                    wait: crate::diagnostics::evidence::WaitKey::from_token(token),
                    carrier: self.id,
                },
            );
        }
        let task = self.in_flight.take().expect("parking task key");
        #[cfg(feature = "runtime-evidence")]
        let task_id = self.task(task).execution().id;
        let reason = self.task(task).execution().data.reason();
        self.shared
            .transition(self.task(task).execution().record(), |record| {
                record.status = TaskStatus::Suspended(reason);
                record.deadline = request.deadline();
                record.parks += 1;
                record.last_suspension = Some(reason);
            });
        self.stats.parks += 1;
        self.parked.insert(token, ParkedTask { task, registration });
        #[cfg(feature = "runtime-evidence")]
        self.shared
            .record(crate::diagnostics::evidence::RuntimeEventKind::Parked {
                task: task_id,
                wait: crate::diagnostics::evidence::WaitKey::from_token(token),
                carrier: self.id,
                reason,
            });
        Ok(())
    }

    fn complete_task(&mut self) {
        self.yield_pressure = 0;
        let task_key = self.in_flight.expect("completed task key");
        let execution = self.execution(task_key);
        let record = Arc::clone(execution.record());
        #[cfg(feature = "lifecycle-profiling")]
        let reclaim_started = std::time::Instant::now();
        #[cfg(feature = "runtime-evidence")]
        let task = record.lock().id;
        {
            let _cleanup = crate::task_context::TaskCleanup::new(execution);
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
#[path = "kernel_drive_test.rs"]
mod kernel_drive_test;
