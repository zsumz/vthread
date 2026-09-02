//! One stationary task dispatch while its slab slot remains borrowed.

use super::Task;
use crate::{context, control::Shared, task_progress::CarrierProgress};
use std::rc::Rc;
use vthread_stack::{FiberState, Suspension};

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
            self.fiber.as_mut().expect("mounted stack").resume()
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
