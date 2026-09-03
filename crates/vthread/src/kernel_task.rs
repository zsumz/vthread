//! One stationary task dispatch while its typed slab slot remains borrowed.

use crate::{
    context::{self, Execution},
    control::Shared,
    kernel_tasks::TaskMut,
    task_progress::CarrierProgress,
};
use std::rc::Rc;
use vthread_stack::{FiberState, Resume, Suspension};

impl TaskMut<'_> {
    #[inline]
    pub(super) fn dispatch(
        &mut self,
        shared: &Shared,
        carrier: &CarrierProgress,
    ) -> Option<FiberState> {
        match self {
            Self::Owned(task) => {
                let fiber = &mut task.fiber;
                let execution = task.execution.as_ref().expect("task execution");
                dispatch_task(execution, shared, carrier, |resume| {
                    fiber.as_mut().expect("mounted stack").resume_with_context(
                        resume,
                        &context::MOUNTED_EXECUTION,
                        execution,
                    )
                })
            }
            Self::Borrowed(task) => {
                let fiber = &mut task.fiber;
                let execution = task.execution.as_ref().expect("task execution");
                dispatch_task(execution, shared, carrier, |resume| {
                    fiber.as_mut().expect("mounted stack").resume_with_context(
                        resume,
                        &context::MOUNTED_EXECUTION,
                        execution,
                    )
                })
            }
        }
    }
}

#[inline]
fn dispatch_task(
    execution: &Rc<Execution>,
    shared: &Shared,
    carrier: &CarrierProgress,
    mut resume_fiber: impl FnMut(Resume) -> Option<FiberState>,
) -> Option<FiberState> {
    let state = {
        // A resumed yield checks policy immediately before returning to the task.
        let mut resume = if execution.progress.resuming_yield() && execution.data.interrupted() {
            Resume::Interrupt
        } else {
            Resume::Continue
        };
        loop {
            let state = resume_fiber(resume)?;
            if !matches!(state, FiberState::Suspended(Suspension::YieldNow)) {
                break Some(state);
            }
            // Check before committing the yield. An interrupted operation resumes
            // immediately, so no other task observes a turn between call and error.
            if execution.data.interrupted() {
                resume = Resume::Interrupt;
                continue;
            }
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
        Some(FiberState::Suspended(Suspension::YieldNow)) => {
            execution.progress.yield_now(carrier, |update| {
                execution.record().progress().apply(update);
            });
        }
        Some(FiberState::Suspended(Suspension::Park(_))) => execution
            .progress
            .park(execution.record().progress(), carrier),
        Some(FiberState::Complete) => {
            execution
                .progress
                .unmount(execution.record().progress(), carrier, execution.id)
        }
        None => {}
    }
    state
}

#[cfg(test)]
#[path = "kernel_task_test.rs"]
mod kernel_task_test;
