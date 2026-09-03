//! Typed carrier-local ownership for static fibers and borrowed fiber leases.

use vthread_stack::{Fiber, FiberLease, FiberState, Resume, StackPool};

pub(crate) struct OwnedFiber {
    fiber: Fiber,
    #[cfg(feature = "runtime-evidence")]
    stack: u64,
}

impl OwnedFiber {
    #[cfg(feature = "runtime-evidence")]
    pub(crate) fn new(fiber: Fiber, stack: u64) -> Self {
        Self { fiber, stack }
    }

    #[cfg(not(feature = "runtime-evidence"))]
    pub(crate) fn new(fiber: Fiber) -> Self {
        Self { fiber }
    }

    pub(crate) fn resume_with(&mut self, resume: Resume) -> Option<FiberState> {
        Some(self.fiber.resume_with(resume))
    }

    #[cfg(feature = "runtime-evidence")]
    fn reclaim_stack(self, pool: &mut StackPool) -> (u64, bool) {
        let retained = pool.release_identified(self.stack, self.fiber.into_stack());
        (self.stack, retained)
    }

    #[cfg(not(feature = "runtime-evidence"))]
    fn reclaim_stack(self, pool: &mut StackPool) {
        pool.release(self.fiber.into_stack());
    }

    #[cfg(feature = "runtime-evidence")]
    fn stack_identity(&self) -> u64 {
        self.stack
    }
}

pub(crate) struct BorrowedFiber {
    fiber: Option<FiberLease>,
    #[cfg(feature = "runtime-evidence")]
    stack: u64,
}

impl BorrowedFiber {
    #[cfg(feature = "runtime-evidence")]
    pub(crate) fn new(fiber: FiberLease, stack: u64) -> Self {
        Self {
            fiber: Some(fiber),
            stack,
        }
    }

    #[cfg(not(feature = "runtime-evidence"))]
    pub(crate) fn new(fiber: FiberLease) -> Self {
        Self { fiber: Some(fiber) }
    }

    pub(crate) fn revoked(&self) -> bool {
        self.fiber.as_ref().is_none_or(|fiber| !fiber.live())
    }

    pub(crate) fn resume_with(&mut self, resume: Resume) -> Option<FiberState> {
        self.fiber.as_ref()?.resume_with(resume)
    }

    #[cfg(feature = "runtime-evidence")]
    fn reclaim_stack(mut self, pool: &mut StackPool) -> (u64, bool) {
        let stack = self.fiber.take().and_then(|fiber| fiber.take_stack());
        if let Some(stack) = stack {
            (self.stack, pool.release_identified(self.stack, stack))
        } else {
            pool.retire(self.stack);
            (self.stack, false)
        }
    }

    #[cfg(not(feature = "runtime-evidence"))]
    fn reclaim_stack(mut self, pool: &mut StackPool) {
        if let Some(stack) = self.fiber.take().and_then(|fiber| fiber.take_stack()) {
            pool.release(stack);
        }
    }

    #[cfg(feature = "runtime-evidence")]
    fn stack_identity(&self) -> u64 {
        self.stack
    }
}

impl Drop for BorrowedFiber {
    fn drop(&mut self) {
        if let Some(fiber) = self.fiber.take() {
            fiber.reclaim();
        }
    }
}

pub(crate) enum TakenFiber {
    Owned(OwnedFiber),
    Borrowed(BorrowedFiber),
}

impl TakenFiber {
    #[cfg(feature = "runtime-evidence")]
    pub(crate) fn stack_identity(&self) -> u64 {
        match self {
            Self::Owned(fiber) => fiber.stack_identity(),
            Self::Borrowed(fiber) => fiber.stack_identity(),
        }
    }

    #[cfg(feature = "runtime-evidence")]
    pub(crate) fn reclaim_stack(self, pool: &mut StackPool) -> (u64, bool) {
        match self {
            Self::Owned(fiber) => fiber.reclaim_stack(pool),
            Self::Borrowed(fiber) => fiber.reclaim_stack(pool),
        }
    }

    #[cfg(not(feature = "runtime-evidence"))]
    pub(crate) fn reclaim_stack(self, pool: &mut StackPool) {
        match self {
            Self::Owned(fiber) => fiber.reclaim_stack(pool),
            Self::Borrowed(fiber) => fiber.reclaim_stack(pool),
        }
    }
}

#[cfg(test)]
#[path = "task_fiber_test.rs"]
mod task_fiber_test;
