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
        self.process_wakes()?;
        for token in self.timers.pop_expired(Instant::now()) {
            if let Some(parked) = self.parked.get(&token) {
                parked.registration.select_timeout(token)?;
            }
        }
        self.process_wakes()?;
        self.in_flight = self.ready.pop_front();
        let Some(task) = &mut self.in_flight else {
            return Ok(false);
        };
        let id = lock(&task.record).id;
        self.shared.transition(&task.record, |record| {
            record.status = TaskStatus::Running;
            record.mounts += 1;
        });
        self.stats.mounts += 1;
        self.publish(CarrierStatus::Running);
        let state = {
            let _mounted = context::mount(id, Arc::clone(&self.inbox.hub));
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
        match state {
            FiberState::Suspended(Suspension::YieldNow) => {
                let task = self.in_flight.take().expect("yielded task");
                self.shared.transition(&task.record, |record| {
                    record.status = TaskStatus::Ready;
                    record.yields += 1;
                    record.last_suspension = Some(SuspensionReason::YieldNow);
                });
                self.stats.yields += 1;
                self.ready.push_back(task);
            }
            FiberState::Suspended(Suspension::Park(request)) => self.park_task(request)?,
            FiberState::Complete => self.complete_task(),
        }
        self.publish(CarrierStatus::Running);
        Ok(true)
    }

    fn process_wakes(&mut self) -> Result<()> {
        while let Some(notice) = self.inbox.hub.pop_wake() {
            let Some(parked) = self.parked.get(&notice.token) else {
                self.stats.stale_wakes += 1;
                continue;
            };
            if lock(&parked.task.record).id != notice.task {
                return Err(Error::Invariant("wake notice task does not own wait token"));
            }
            let parked = self.parked.remove(&notice.token).expect("validated park");
            self.timers.cancel(notice.token);
            self.shared.transition(&parked.task.record, |record| {
                record.status = TaskStatus::Ready;
                record.deadline = None;
                record.last_wake = Some(match notice.cause {
                    WakeCause::Ready => WakeReason::Ready,
                    WakeCause::TimedOut => WakeReason::TimedOut,
                    WakeCause::Cancelled => WakeReason::Cancelled,
                    WakeCause::Closed => WakeReason::Closed,
                });
            });
            self.stats.wakes += 1;
            match notice.cause {
                WakeCause::Ready => {}
                WakeCause::TimedOut => self.stats.timeouts += 1,
                WakeCause::Cancelled => self.stats.cancelled += 1,
                WakeCause::Closed => self.stats.closed += 1,
            }
            self.ready.push_back(parked.task);
        }
        Ok(())
    }

    fn park_task(&mut self, request: ParkRequest) -> Result<()> {
        let token = request.token();
        if self.parked.contains_key(&token) {
            return Err(Error::Invariant("wait token parked twice"));
        }
        let registration = self.inbox.hub.take_registration(token)?;
        if let Some(deadline) = request.deadline()
            && !self.timers.schedule(token, deadline)
        {
            return Err(Error::Invariant("wait timer scheduled twice"));
        }
        let task = self.in_flight.take().expect("parking task");
        self.shared.transition(&task.record, |record| {
            record.status = TaskStatus::Suspended(SuspensionReason::Park);
            record.deadline = request.deadline();
            record.parks += 1;
            record.last_suspension = Some(SuspensionReason::Park);
        });
        self.stats.parks += 1;
        self.parked.insert(token, ParkedTask { task, registration });
        Ok(())
    }

    fn complete_task(&mut self) {
        let task = self.in_flight.as_mut().expect("completed task");
        let stack = task.fiber.take().expect("completed stack").into_stack();
        self.stacks.release(stack);
        let record = Arc::clone(&task.record);
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
