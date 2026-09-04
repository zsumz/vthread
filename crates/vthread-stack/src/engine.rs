//! The fiber lifecycle state machine over the transfer protocol.

use std::{marker::PhantomData, panic::resume_unwind, ptr::NonNull};

use crate::{
    FiberState, MappedStack, Resume, Suspension, arch,
    context::{Command, FiberCore, ForcedUnwind, Outcome},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    /// The entry has not run; its closure still waits at the top of the stack.
    Unstarted,
    /// Frames are live on the fiber stack, parked inside `suspend`.
    Suspended,
    /// The root has finished; the stack holds no live frames.
    Terminal,
}

/// One fiber: the control block at the top of its own mapped stack plus its stage.
pub(crate) struct Execution {
    core: NonNull<FiberCore>,
    stack: Option<MappedStack>,
    stage: Stage,
    // A started fiber never migrates: its saved context belongs to one carrier.
    carrier_local: PhantomData<*mut ()>,
}

impl Execution {
    /// Places the control block and `entry` on `stack` and fabricates the first frame.
    ///
    /// # Safety
    ///
    /// The caller must reclaim this execution before the entry's borrows expire.
    pub(crate) unsafe fn start<F: FnOnce()>(stack: MappedStack, entry: F) -> Self {
        // SAFETY: the stack was just handed over, so nothing is live on it.
        let placement = unsafe { FiberCore::place(&stack, entry) };
        // SAFETY: `place` leaves at least one frame plus headroom below `frame_top`, and
        // the block stays valid for as long as this execution owns the stack.
        let child_sp = unsafe { arch::init_frame(placement.frame_top, placement.core) };
        let execution = Self {
            core: placement.core,
            stack: Some(stack),
            stage: Stage::Unstarted,
            carrier_local: PhantomData,
        };
        execution.core().set_child_sp(child_sp);
        execution
    }

    /// The control block this fiber suspends through; stable while the execution lives.
    pub(crate) fn core_ptr(&self) -> *const FiberCore {
        self.core.as_ptr()
    }

    fn core(&self) -> &FiberCore {
        // SAFETY: the block lives on the stack this execution owns until `into_stack`
        // consumes the execution or `Drop` finishes unwinding it.
        unsafe { self.core.as_ref() }
    }

    /// Runs until the next suspension or completion; the caller installs the mount.
    #[inline]
    pub(crate) fn resume(&mut self, resume: Resume) -> FiberState {
        assert!(
            self.stage != Stage::Terminal,
            "a completed fiber cannot be resumed"
        );
        self.core().issue(Command::Resume(resume));
        self.stage = Stage::Suspended;
        self.transfer();
        match self.core().take_outcome() {
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

    #[inline]
    fn transfer(&mut self) {
        let core = self.core();
        // SAFETY: the child slot holds the fabricated first frame or a context saved by
        // `suspend` on this carrier, and the parent slot receives this context.
        unsafe { arch::context_switch(core.parent_sp_slot(), core.child_sp()) };
    }

    /// Drives a non-terminal fiber to its root, running every live destructor.
    fn force_unwind(&mut self) {
        match self.stage {
            Stage::Terminal => return,
            Stage::Unstarted => {
                drop(self.core().take_entry());
                self.stage = Stage::Terminal;
                return;
            }
            Stage::Suspended => {}
        }
        loop {
            self.core()
                .issue(Command::ForceUnwind(self.core().cookie()));
            self.transfer();
            match self.core().take_outcome() {
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

/// Parks the mounted fiber and returns the carrier's next decision.
///
/// # Safety
///
/// `core` must belong to the currently mounted, non-Send execution on this carrier.
#[inline]
pub(crate) unsafe fn suspend(core: *const FiberCore, reason: Suspension) -> Resume {
    // SAFETY: the caller guarantees the block belongs to the mounted fiber on this carrier.
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
#[path = "engine_test.rs"]
mod engine_test;
