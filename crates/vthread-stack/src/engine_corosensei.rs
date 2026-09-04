//! Interim engine: corosensei context switching on vthread-owned stack mappings.

use std::ptr::{self, NonNull};

use corosensei::{Coroutine, CoroutineResult};

use crate::{
    FiberState, MappedStack, Resume, Suspension,
    mount::{YielderMount, mounted_yielder},
};

/// The switch handle a mounted corosensei fiber suspends through.
pub(crate) type Yielder = corosensei::Yielder<Resume, Suspension>;

pub(crate) struct Execution {
    coroutine: Option<Coroutine<Resume, Suspension, (), MappedStack>>,
    yielder: NonNull<Yielder>,
}

impl Execution {
    /// The caller must reclaim this execution before the entry's borrows expire.
    pub(crate) unsafe fn start<F: FnOnce()>(stack: MappedStack, entry: F) -> Self {
        // SAFETY: the caller owns the entry's lifetime and guarantees reclamation.
        let coroutine = unsafe {
            Coroutine::with_stack_unchecked(stack, move |current, _resume| {
                // The mount stores the selected engine's handle type; when this engine
                // is not selected the cast only keeps the dead code type-correct.
                let pointer = (current as *const Yielder).cast();
                let _mount = YielderMount::install(pointer);
                entry();
            })
        };
        Self {
            coroutine: Some(coroutine),
            yielder: NonNull::dangling(),
        }
    }

    /// The mounted switch handle, or null until the first suspension reveals it.
    pub(crate) fn yielder(&self) -> *const Yielder {
        if self.yielder == NonNull::dangling() {
            ptr::null()
        } else {
            self.yielder.as_ptr()
        }
    }

    /// Runs until the next suspension or completion; the caller installs the mount.
    pub(crate) fn resume(&mut self, resume: Resume) -> FiberState {
        let coroutine = self
            .coroutine
            .as_mut()
            .expect("a completed fiber cannot be resumed");
        let state = match coroutine.resume(resume) {
            CoroutineResult::Yield(reason) => FiberState::Suspended(reason),
            CoroutineResult::Return(()) => FiberState::Complete,
        };
        match &state {
            FiberState::Suspended(_) if self.yielder == NonNull::dangling() => {
                // The yielder lives on this coroutine's owned stack. Its entry
                // installed the pointer before the first possible suspension.
                let yielder = mounted_yielder().cast::<Yielder>();
                self.yielder =
                    NonNull::new(yielder.cast_mut()).expect("suspended fiber has no yielder");
            }
            FiberState::Suspended(_) => {}
            FiberState::Complete => self.yielder = NonNull::dangling(),
        }
        state
    }

    /// Returns whether the entry function has completed.
    pub(crate) fn is_complete(&self) -> bool {
        self.coroutine
            .as_ref()
            .is_none_or(|coroutine| coroutine.done())
    }

    /// Reclaims the stack; the caller guarantees completion.
    pub(crate) fn into_stack(mut self) -> MappedStack {
        self.coroutine
            .take()
            .expect("fiber stack already extracted")
            .into_stack()
    }
}

/// Suspends the mounted corosensei fiber and returns the carrier's next decision.
///
/// # Safety
///
/// `yielder` must be the live handle of the currently mounted, non-Send coroutine.
pub(crate) unsafe fn suspend(yielder: *const Yielder, reason: Suspension) -> Resume {
    // SAFETY: the caller guarantees the handle belongs to the mounted coroutine.
    unsafe { (*yielder).suspend(reason) }
}

#[cfg(test)]
#[path = "engine_corosensei_test.rs"]
mod engine_corosensei_test;
