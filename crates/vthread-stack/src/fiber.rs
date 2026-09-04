//! Carrier-local stackful fiber wrapper over the selected stack engine.

use crate::{
    FiberState, MappedStack, Resume, engine,
    mount::{ContextSlot, MountGuard, YielderMount},
};

/// One carrier-local stackful execution context.
///
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<vthread_stack::Fiber>();
/// ```
pub struct Fiber {
    execution: Option<engine::Execution>,
}

impl Fiber {
    /// Creates a fiber on an already allocated stack.
    pub fn new<F>(stack: MappedStack, entry: F) -> Self
    where
        F: FnOnce() + 'static,
    {
        // SAFETY: a static entry has no borrowed environment to outlive.
        unsafe { Self::borrowed(stack, entry) }
    }

    /// The caller must reclaim this fiber before the entry's borrows expire.
    pub(crate) unsafe fn borrowed<F: FnOnce()>(stack: MappedStack, entry: F) -> Self {
        // SAFETY: the caller owns the entry's lifetime and guarantees reclamation.
        let execution = unsafe { engine::Execution::start(stack, entry) };
        Self {
            execution: Some(execution),
        }
    }

    /// Mounts the fiber until it suspends or completes.
    ///
    /// Panics if the fiber has already completed, without mounting its old yielder.
    pub fn resume(&mut self) -> FiberState {
        self.resume_with(Resume::Continue)
    }

    /// Mounts the fiber and delivers a decision to its last suspension point.
    pub fn resume_with(&mut self, resume: Resume) -> FiberState {
        self.resume_mounted(resume, None)
    }

    /// Mounts the fiber with typed runtime context for this resume only.
    #[doc(hidden)]
    #[inline]
    pub fn resume_with_context<T>(
        &mut self,
        resume: Resume,
        key: &'static crate::ContextKey<T>,
        value: &T,
    ) -> FiberState {
        let context = ContextSlot::new(key, value);
        self.resume_mounted(resume, Some(&context))
    }

    #[inline]
    fn resume_mounted(&mut self, resume: Resume, context: Option<&ContextSlot<'_>>) -> FiberState {
        // Reject the transition before a panic hook can observe a stale mount.
        assert!(!self.is_complete(), "a completed fiber cannot be resumed");
        let execution = self
            .execution
            .as_mut()
            .expect("a completed fiber cannot be resumed");
        let _mount = MountGuard::install(execution.yielder(), context);
        execution.resume(resume)
    }

    /// Returns whether the entry function has completed.
    pub fn is_complete(&self) -> bool {
        self.execution
            .as_ref()
            .is_none_or(|execution| execution.is_complete())
    }

    /// Reclaims the stack after completion so it can be reused.
    ///
    /// Panics if incomplete; the fiber is still reclaimed with its yielder mounted.
    pub fn into_stack(mut self) -> MappedStack {
        // Keep ownership here on failure so Drop mounts the fiber for unwinding.
        assert!(
            self.is_complete(),
            "an incomplete fiber cannot be extracted"
        );
        self.execution
            .take()
            .expect("fiber stack already extracted")
            .into_stack()
    }
}

impl Drop for Fiber {
    fn drop(&mut self) {
        if let Some(execution) = self.execution.take() {
            let _mount = YielderMount::install(execution.yielder());
            drop(execution);
        }
    }
}

#[cfg(test)]
#[path = "fiber_test.rs"]
mod fiber_test;
