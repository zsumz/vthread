//! Uniform carrier-local ownership for static and lexically borrowed fibers.

use vthread_stack::{Fiber, FiberLease, FiberState};

pub(crate) enum TaskFiber {
    Owned {
        fiber: Option<Fiber>,
        #[cfg(feature = "runtime-evidence")]
        stack: u64,
    },
    Borrowed {
        fiber: FiberLease,
        #[cfg(feature = "runtime-evidence")]
        stack: u64,
    },
}

impl TaskFiber {
    #[cfg(feature = "runtime-evidence")]
    pub(crate) fn owned(fiber: Fiber, stack: u64) -> Self {
        Self::Owned {
            fiber: Some(fiber),
            stack,
        }
    }

    #[cfg(not(feature = "runtime-evidence"))]
    pub(crate) fn owned(fiber: Fiber) -> Self {
        Self::Owned { fiber: Some(fiber) }
    }

    #[cfg(feature = "runtime-evidence")]
    pub(crate) fn borrowed(fiber: FiberLease, stack: u64) -> Self {
        Self::Borrowed { fiber, stack }
    }

    #[cfg(not(feature = "runtime-evidence"))]
    pub(crate) fn borrowed(fiber: FiberLease) -> Self {
        Self::Borrowed { fiber }
    }

    #[cfg(feature = "runtime-evidence")]
    pub(crate) fn stack_identity(&self) -> u64 {
        match self {
            Self::Owned { stack, .. } | Self::Borrowed { stack, .. } => *stack,
        }
    }

    pub(crate) fn revoked(&self) -> bool {
        matches!(self, Self::Borrowed { fiber, .. } if !fiber.live())
    }

    pub(crate) fn resume(&mut self) -> Option<FiberState> {
        match self {
            Self::Owned { fiber, .. } => Some(fiber.as_mut().expect("owned fiber").resume()),
            Self::Borrowed { fiber, .. } => fiber.resume(),
        }
    }

    #[cfg(feature = "runtime-evidence")]
    pub(crate) fn reclaim_stack(&mut self, pool: &mut vthread_stack::StackPool) -> (u64, bool) {
        let identity = self.stack_identity();
        let stack = match self {
            Self::Owned { fiber, .. } => fiber.take().map(Fiber::into_stack),
            Self::Borrowed { fiber, .. } => fiber.take_stack(),
        };
        if let Some(stack) = stack {
            (identity, pool.release_identified(identity, stack))
        } else {
            pool.retire(identity);
            (identity, false)
        }
    }

    #[cfg(not(feature = "runtime-evidence"))]
    pub(crate) fn reclaim_stack(&mut self, pool: &mut vthread_stack::StackPool) {
        let stack = match self {
            Self::Owned { fiber, .. } => fiber.take().map(Fiber::into_stack),
            Self::Borrowed { fiber, .. } => fiber.take_stack(),
        };
        if let Some(stack) = stack {
            pool.release(stack);
        }
    }
}

impl Drop for TaskFiber {
    fn drop(&mut self) {
        if let Self::Borrowed { fiber, .. } = self {
            fiber.reclaim();
        }
    }
}

#[cfg(test)]
#[path = "task_fiber_test.rs"]
mod task_fiber_test;
