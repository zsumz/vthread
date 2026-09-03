//! One stationary task dispatch while its slab slot remains borrowed.

use super::Task;
use crate::{context, control::Shared, task_progress::CarrierProgress};
use std::rc::Rc;
use vthread_stack::{FiberState, Resume, Suspension};

impl Task {
    #[inline]
    pub(super) fn dispatch(
        &mut self,
        shared: &Shared,
        carrier: &CarrierProgress,
    ) -> Option<FiberState> {
        let state = {
            let execution = Rc::clone(&self.execution);
            let _mounted = context::mount_execution(execution);
            // A resumed yield checks policy immediately before returning to the task.
            let mut resume = if std::mem::take(&mut self.checkpoint_on_resume)
                && self.execution.data.interrupted()
            {
                Resume::Interrupt
            } else {
                Resume::Continue
            };
            loop {
                let state = self
                    .fiber
                    .as_mut()
                    .expect("mounted stack")
                    .resume_with(resume)?;
                if !matches!(state, FiberState::Suspended(Suspension::YieldNow)) {
                    break Some(state);
                }
                // Check before committing the yield. An interrupted operation resumes
                // immediately, so no other task observes a turn between call and error.
                if self.execution.data.interrupted() {
                    resume = Resume::Interrupt;
                    continue;
                }
                self.checkpoint_on_resume = true;
                break Some(state);
            }
        };
        #[cfg(test)]
        if shared
            .fail_after_resume
            .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            panic!("injected scheduler failure after resume");
        }
        #[cfg(not(test))]
        let _ = shared;
        match &state {
            Some(FiberState::Suspended(Suspension::YieldNow)) => self
                .execution
                .progress
                .yield_now(self.execution.record.progress(), carrier),
            Some(FiberState::Suspended(Suspension::Park(_))) => self
                .execution
                .progress
                .park(self.execution.record.progress(), carrier),
            Some(FiberState::Complete) => self.execution.progress.unmount(
                self.execution.record.progress(),
                carrier,
                self.execution.id,
            ),
            None => {}
        }
        state
    }
}

#[cfg(test)]
#[path = "kernel_task_test.rs"]
mod kernel_task_test;
