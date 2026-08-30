//! Scheduler driving, parking, wake delivery, and timer progression.

use std::{rc::Rc, thread, time::Instant};

use vthread_stack::{FiberState, ParkRequest, Suspension};

use crate::{
    Error, Result, SuspensionReason, TaskStatus, WakeReason,
    context,
    wait::{WakeCause, WakeNotice},
};

use super::{Kernel, ParkedTask, Task};

impl Kernel {
    pub(crate) fn tick(&mut self) -> Result<bool> {
        self.prepare_runnable()?;
        let Some(mut task) = self.ready.pop_front() else {
            return Ok(false);
        };
        {
            let mut record = task.record.borrow_mut();
            record.status = TaskStatus::Running;
            record.mounts += 1;
        }
        self.stats.mounts += 1;
        let task_id = task.record.borrow().id;
        let _mounted = context::mount(task_id, Rc::clone(&self.hub));

        match task.fiber.resume() {
            FiberState::Suspended(Suspension::YieldNow) => self.requeue_yield(task),
            FiberState::Suspended(Suspension::Park(request)) => self.park_task(task, request)?,
            FiberState::Complete => self.complete_task(task)?,
        }
        Ok(true)
    }

    fn prepare_runnable(&mut self) -> Result<()> {
        loop {
            self.process_wakes()?;
            self.expire_timers()?;
            self.process_wakes()?;
            if !self.ready.is_empty() {
                return Ok(());
            }
            let Some(deadline) = self.timers.next_deadline() else {
                return Ok(());
            };
            let delay = deadline.saturating_duration_since(Instant::now());
            if !delay.is_zero() {
                self.stats.timer_sleeps += 1;
                thread::sleep(delay);
            }
        }
    }

    fn expire_timers(&mut self) -> Result<()> {
        for token in self.timers.pop_expired(Instant::now()) {
            let Some(parked) = self.parked.get(&token) else {
                continue;
            };
            parked.registration.select_timeout(token)?;
        }
        Ok(())
    }

    fn process_wakes(&mut self) -> Result<()> {
        while let Some(notice) = self.hub.pop_wake() {
            self.process_wake(notice)?;
        }
        Ok(())
    }

    fn process_wake(&mut self, notice: WakeNotice) -> Result<()> {
        let Some(parked_task) = self.parked.get(&notice.token) else {
            self.stats.stale_wakes += 1;
            return Ok(());
        };
        if parked_task.task.record.borrow().id != notice.task {
            return Err(Error::Invariant("wake notice task does not own wait token"));
        }
        let parked = self
            .parked
            .remove(&notice.token)
            .ok_or(Error::Invariant("validated parked task disappeared"))?;
        self.timers.cancel(notice.token);
        {
            let mut record = parked.task.record.borrow_mut();
            record.status = TaskStatus::Ready;
            record.last_wake = Some(wake_reason(notice.cause));
        }
        self.stats.wakes += 1;
        match notice.cause {
            WakeCause::Ready => {}
            WakeCause::TimedOut => self.stats.timeouts += 1,
            WakeCause::Cancelled => self.stats.cancelled += 1,
            WakeCause::Closed => self.stats.closed += 1,
        }
        self.ready.push_back(parked.task);
        Ok(())
    }

    fn requeue_yield(&mut self, task: Task) {
        {
            let mut record = task.record.borrow_mut();
            record.status = TaskStatus::Ready;
            record.yields += 1;
            record.last_suspension = Some(SuspensionReason::YieldNow);
        }
        self.stats.yields += 1;
        self.ready.push_back(task);
    }

    fn park_task(&mut self, task: Task, request: ParkRequest) -> Result<()> {
        let token = request.token();
        if self.parked.contains_key(&token) {
            return Err(Error::Invariant("wait token parked twice"));
        }
        let registration = self.hub.take_registration(token)?;
        if let Some(deadline) = request.deadline() {
            if !self.timers.schedule(token, deadline) {
                return Err(Error::Invariant("wait timer scheduled twice"));
            }
        }
        {
            let mut record = task.record.borrow_mut();
            record.status = TaskStatus::Suspended(SuspensionReason::Park);
            record.parks += 1;
            record.last_suspension = Some(SuspensionReason::Park);
        }
        self.stats.parks += 1;
        let previous = self
            .parked
            .insert(token, ParkedTask { task, registration });
        debug_assert!(previous.is_none(), "park token was checked before insertion");
        Ok(())
    }

    fn complete_task(&mut self, task: Task) -> Result<()> {
        let status = task.record.borrow().status;
        if status == TaskStatus::Running {
            task.record.borrow_mut().status = TaskStatus::Completed;
        }
        match task.record.borrow().status {
            TaskStatus::Completed => self.stats.completed += 1,
            TaskStatus::Panicked => self.stats.panicked += 1,
            _ => return Err(Error::Invariant("completed fiber has nonterminal task state")),
        }
        self.active = self
            .active
            .checked_sub(1)
            .ok_or(Error::Invariant("active task count underflow"))?;
        self.stacks.release(task.fiber.into_stack());
        Ok(())
    }
}

fn wake_reason(cause: WakeCause) -> WakeReason {
    match cause {
        WakeCause::Ready => WakeReason::Ready,
        WakeCause::TimedOut => WakeReason::TimedOut,
        WakeCause::Cancelled => WakeReason::Cancelled,
        WakeCause::Closed => WakeReason::Closed,
    }
}

#[cfg(test)]
#[path = "kernel_drive_test.rs"]
mod kernel_drive_test;
