//! Uniform carrier-local ownership for static and lexically borrowed fibers.

use vthread_stack::{Fiber, FiberLease, FiberState};

pub(crate) enum TaskFiber {
    Owned(Option<Fiber>),
    Borrowed(FiberLease),
}

impl TaskFiber {
    pub(crate) fn revoked(&self) -> bool {
        matches!(self, Self::Borrowed(fiber) if !fiber.live())
    }

    pub(crate) fn resume(&mut self) -> Option<FiberState> {
        match self {
            Self::Owned(fiber) => Some(fiber.as_mut().expect("owned fiber").resume()),
            Self::Borrowed(fiber) => fiber.resume(),
        }
    }

    pub(crate) fn reclaim_stack(&mut self, pool: &mut vthread_stack::StackPool) {
        let stack = match self {
            Self::Owned(fiber) => fiber.take().map(Fiber::into_stack),
            Self::Borrowed(fiber) => fiber.take_stack(),
        };
        if let Some(stack) = stack {
            pool.release(stack);
        }
    }
}

impl Drop for TaskFiber {
    fn drop(&mut self) {
        if let Self::Borrowed(fiber) = self {
            fiber.reclaim();
        }
    }
}

#[cfg(test)]
#[path = "task_fiber_test.rs"]
mod task_fiber_test;
