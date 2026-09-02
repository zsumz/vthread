//! Owner-only mounting, generation-checked waking, and timer progression.

use super::{Kernel, ParkedTask};
use crate::{
    CarrierStatus, Error, Result, SuspensionReason, TaskStatus, WakeReason, context, signal::lock,
    wait::WakeCause,
};
use std::{sync::Arc, time::Instant};
use vthread_stack::{FiberState, ParkRequest, Suspension};

impl Kernel {
    pub(crate) fn tick(&mut self) -> Result<bool> {
        self.sweep_revoked();
        self.process_wakes()?;
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
        let Some(task) = &mut self.in_flight else {
            return Ok(false);
        };
        if let Some(reason) = self.shared.abort_reason(task.execution.scope) {
            self.discard_in_flight(reason);
            return Ok(true);
        }
        let first_mount = task.execution.progress.mount();
        if self.shared.config.stall_policy().timeout().is_some() {
            self.shared.transition(&task.execution.record, |record| {
                record.status = TaskStatus::Running;
            });
        }
        #[cfg(feature = "runtime-evidence")]
        self.shared
            .record(crate::diagnostics::evidence::RuntimeEventKind::Mounted {
                task: task.execution.id,
                carrier: self.id,
            });
        self.stats.mounts += 1;
        if first_mount {
            self.publish(CarrierStatus::Running);
        }
        let state = {
            let execution = self.execution(self.in_flight.as_ref().expect("mounted task"));
            let _mounted = context::mount_execution(execution);
            self.in_flight
                .as_mut()
                .expect("mounted task")
                .fiber
                .as_mut()
                .expect("mounted stack")
                .resume()
        };
        #[cfg(test)]
        if self
            .shared
            .fail_after_resume
            .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            panic!("injected scheduler failure after resume");
        }
        let publish = match state {
            Some(FiberState::Suspended(Suspension::YieldNow)) => {
                let task = self.in_flight.take().expect("yielded task");
                #[cfg(feature = "runtime-evidence")]
                let task_id = task.execution.id;
                task.execution.progress.yield_now();
                if self.shared.config.stall_policy().timeout().is_some() {
                    self.shared.transition(&task.execution.record, |record| {
                        record.status = TaskStatus::Ready;
                        record.last_suspension = Some(SuspensionReason::YieldNow);
                    });
                }
                self.stats.yields += 1;
                #[cfg(feature = "runtime-evidence")]
                self.shared
                    .record(crate::diagnostics::evidence::RuntimeEventKind::Yielded {
                        task: task_id,
                        carrier: self.id,
                    });
                self.ready.push_back(task);
                false
            }
            Some(FiberState::Suspended(Suspension::Park(request))) => {
                self.in_flight
                    .as_ref()
                    .expect("parking task")
                    .execution
                    .progress
                    .unmount();
                self.park_task(request)?;
                true
            }
            Some(FiberState::Complete) => {
                self.in_flight
                    .as_ref()
                    .expect("completed task")
                    .execution
                    .progress
                    .unmount();
                self.complete_task();
                false
            }
            None => {
                self.in_flight
                    .as_ref()
                    .expect("revoked task")
                    .execution
                    .progress
                    .unmount();
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
            if parked.task.execution.id != notice.task {
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
                .transition(&parked.task.execution.record, |record| {
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
        let task = self.in_flight.take().expect("parking task");
        #[cfg(feature = "runtime-evidence")]
        let task_id = task.execution.id;
        let reason = task.execution.data.reason.get();
        task.execution.progress.clear_yield();
        self.shared.transition(&task.execution.record, |record| {
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
        let execution = self.execution(self.in_flight.as_ref().expect("completed task"));
        let record = Arc::clone(&execution.record);
        #[cfg(feature = "runtime-evidence")]
        let task = lock(&record).id;
        {
            let _cleanup = crate::task_context::TaskCleanup::new(execution);
            #[cfg(feature = "runtime-evidence")]
            let (identity, retained) = self
                .in_flight
                .as_mut()
                .expect("completed task")
                .fiber
                .take()
                .expect("completed stack")
                .reclaim_stack(&mut self.local.stacks.borrow_mut());
            #[cfg(not(feature = "runtime-evidence"))]
            self.in_flight
                .as_mut()
                .expect("completed task")
                .fiber
                .take()
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
        if lock(&record).panic.is_some() {
            self.stats.panicked += 1;
        } else {
            self.stats.completed += 1;
        }
        self.in_flight = None;
        self.publish(CarrierStatus::Running);
        // Completion and admission release become visible only after reclaiming the stack.
        self.shared.complete(&record, None);
    }

    pub(crate) fn wait_for_work(&mut self, observed: u64) {
        let deadline = self.timers.next_deadline();
        if deadline.is_some() {
            self.stats.timer_sleeps += 1;
        }
        self.publish(CarrierStatus::Idle);
        self.inbox.signal.wait(observed, deadline);
    }
}

#[cfg(test)]
#[path = "kernel_drive_test.rs"]
mod kernel_drive_test;
