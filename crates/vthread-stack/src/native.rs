//! Native stackful execution: the lifecycle state machine over the transfer protocol.

use std::{marker::PhantomData, panic::resume_unwind, ptr::NonNull};

use crate::{
    FiberState, MappedStack, Resume, Suspension, arch,
    context::{Command, FiberCore, ForcedUnwind, Outcome},
    entry::ErasedEntry,
};

/// The mounted handle a native fiber suspends through.
pub(crate) type Yielder = FiberCore;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    /// The entry has not run; its closure still lives on the carrier heap.
    Unstarted,
    /// Frames are live on the fiber stack, parked inside `suspend`.
    Suspended,
    /// The root has finished; the stack holds no live frames.
    Terminal,
}

pub(crate) struct Execution {
    core: Box<FiberCore>,
    stack: Option<MappedStack>,
    stage: Stage,
    // A started fiber never migrates: its saved context belongs to one carrier.
    carrier_local: PhantomData<*mut ()>,
}

impl Execution {
    /// The caller must reclaim this execution before the entry's borrows expire.
    pub(crate) unsafe fn start<F: FnOnce()>(stack: MappedStack, entry: F) -> Self {
        let core = Box::new(FiberCore::new(ErasedEntry::new(entry)));
        // SAFETY: the mapping is live and its usable range holds at least one page,
        // which exceeds the first frame; the boxed core outlives every fiber frame.
        let child_sp = unsafe { arch::init_frame(&stack, NonNull::from(&*core)) };
        core.set_child_sp(child_sp);
        Self {
            core,
            stack: Some(stack),
            stage: Stage::Unstarted,
            carrier_local: PhantomData,
        }
    }

    /// The core this fiber suspends through; stable from creation.
    pub(crate) fn yielder(&self) -> *const FiberCore {
        &*self.core
    }

    /// Runs until the next suspension or completion; the caller installs the mount.
    pub(crate) fn resume(&mut self, resume: Resume) -> FiberState {
        assert!(
            self.stage != Stage::Terminal,
            "a completed fiber cannot be resumed"
        );
        self.core.issue(Command::Resume(resume));
        self.stage = Stage::Suspended;
        self.transfer();
        match self.core.take_outcome() {
            Outcome::Suspended(reason) => FiberState::Suspended(reason),
            Outcome::Complete => {
                self.stage = Stage::Terminal;
                FiberState::Complete
            }
            Outcome::Panicked(payload) => {
                self.stage = Stage::Terminal;
                resume_unwind(payload)
            }
            Outcome::Unwound => unreachable!("a fiber unwound without a forced-unwind command"),
        }
    }

    /// Returns whether the fiber holds no live frames and can never run again.
    pub(crate) fn is_complete(&self) -> bool {
        self.stage == Stage::Terminal
    }

    /// Reclaims the stack; the caller guarantees completion.
    pub(crate) fn into_stack(mut self) -> MappedStack {
        assert!(
            self.stage == Stage::Terminal,
            "an incomplete fiber cannot be extracted"
        );
        self.stack.take().expect("fiber stack already extracted")
    }

    fn transfer(&mut self) {
        // SAFETY: the child slot holds the fabricated first frame or a context saved by
        // `suspend` on this carrier, and the parent slot receives this context.
        unsafe { arch::context_switch(self.core.parent_sp_slot(), self.core.child_sp()) };
    }

    /// Drives a non-terminal fiber to its root, running every live destructor.
    fn force_unwind(&mut self) {
        match self.stage {
            Stage::Terminal => return,
            Stage::Unstarted => {
                drop(self.core.take_entry());
                self.stage = Stage::Terminal;
                return;
            }
            Stage::Suspended => {}
        }
        loop {
            self.core.issue(Command::ForceUnwind(self.core.cookie()));
            self.transfer();
            match self.core.take_outcome() {
                // User code caught the token and suspended again; inject it again.
                Outcome::Suspended(_) => {}
                Outcome::Complete | Outcome::Unwound => {
                    self.stage = Stage::Terminal;
                    return;
                }
                Outcome::Panicked(payload) => {
                    self.stage = Stage::Terminal;
                    resume_unwind(payload);
                }
            }
        }
    }
}

impl Drop for Execution {
    fn drop(&mut self) {
        // The mapping is released only once the fiber holds no live frames.
        self.force_unwind();
    }
}

/// Parks the mounted native fiber and returns the carrier's next decision.
///
/// # Safety
///
/// `core` must belong to the currently mounted, non-Send execution on this carrier.
pub(crate) unsafe fn suspend(core: *const FiberCore, reason: Suspension) -> Resume {
    // SAFETY: the caller guarantees the core belongs to the mounted fiber on this carrier.
    let core = unsafe { &*core };
    core.report(Outcome::Suspended(reason));
    // SAFETY: the carrier saved its context in the parent slot before resuming this
    // fiber, and the child slot receives this context for the switch back.
    unsafe { arch::context_switch(core.child_sp_slot(), core.parent_sp()) };
    match core.command() {
        Command::Resume(resume) => resume,
        Command::ForceUnwind(cookie) => resume_unwind(Box::new(ForcedUnwind::new(cookie))),
    }
}

#[cfg(test)]
#[path = "native_test.rs"]
mod native_test;
