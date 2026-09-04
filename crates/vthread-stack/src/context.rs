//! Transfer protocol between a carrier and one fiber, and the fiber's control block.
//!
//! The control block and the entry closure live at the top of the fiber's own stack:
//!
//! ```text
//! base ─┬─ FiberCore      control block, stable for the fiber's whole life
//!       ├─ entry storage  the closure, moved out by the root on first entry
//!       ├─ frame_top      sixteen-byte aligned
//!       ├─ first frame    written by `arch::init_frame`, consumed on the first resume
//!       │  ...            ordinary frames grow downward from here
//!       └─ guard page
//! ```
//!
//! A fiber started on a pooled stack therefore performs no heap allocation.

use std::{
    any::Any,
    cell::Cell,
    mem,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr::NonNull,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{MappedStack, Resume, STACK_ALIGNMENT, Suspension, arch, entry::ErasedEntry};

/// Ordinary frames below the first one always get at least this much room.
const MIN_HEADROOM: usize = 4096;

/// What the carrier asks of a fiber when it hands control over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    /// Return from the pending suspension with this decision.
    Resume(Resume),
    /// Unwind every live frame back to the fiber root using this cookie.
    ForceUnwind(u64),
}

/// What a fiber reports when it hands control back.
pub(crate) enum Outcome {
    /// The fiber parked inside `suspend` with this reason.
    Suspended(Suspension),
    /// The entry returned normally.
    Complete,
    /// The entry unwound with a payload the carrier must rethrow.
    Panicked(Box<dyn Any + Send>),
    /// The fiber's own forced-unwind token reached its root.
    Unwound,
}

/// Payload of the private panic that unwinds a suspended fiber back to its root.
///
/// Only the root whose cookie matches suppresses it. Every other observer treats it
/// as an ordinary panic, so one fiber's token can never silently terminate another.
pub(crate) struct ForcedUnwind {
    cookie: u64,
}

impl ForcedUnwind {
    pub(crate) fn new(cookie: u64) -> Self {
        Self { cookie }
    }
}

static NEXT_COOKIE: AtomicU64 = AtomicU64::new(1);

/// Where a fiber's control block and first frame sit on its stack.
pub(crate) struct Placement {
    pub(crate) core: NonNull<FiberCore>,
    /// Aligned address the first saved context is written below.
    pub(crate) frame_top: usize,
}

/// State both stacks share, resident at the top of the fiber's own stack.
pub(crate) struct FiberCore {
    parent_sp: Cell<usize>,
    child_sp: Cell<usize>,
    command: Cell<Command>,
    outcome: Cell<Option<Outcome>>,
    entry: Cell<Option<ErasedEntry>>,
    cookie: u64,
}

impl FiberCore {
    /// Writes a control block holding `entry` at the top of `stack`.
    ///
    /// Panics when the stack cannot hold the entry, the first frame, and minimal headroom.
    ///
    /// # Safety
    ///
    /// No fiber may be live on `stack`. The placement stays valid until the mapping is
    /// released or another placement overwrites it.
    pub(crate) unsafe fn place<F: FnOnce()>(stack: &MappedStack, entry: F) -> Placement {
        let core = align_down(
            stack.base().get() - mem::size_of::<Self>(),
            mem::align_of::<Self>(),
        );
        let storage = align_down(core - mem::size_of::<F>(), mem::align_of::<F>());
        let frame_top = align_down(storage, STACK_ALIGNMENT);
        let lowest = stack.limit().get() + stack.guard_len() + arch::FRAME_LEN + MIN_HEADROOM;
        assert!(
            frame_top >= lowest,
            "a {} byte fiber entry does not fit on a {} byte stack",
            mem::size_of::<F>(),
            stack.usable_len()
        );
        let storage = NonNull::new(storage as *mut ()).expect("stack addresses are never null");
        let core = NonNull::new(core as *mut Self).expect("stack addresses are never null");
        // SAFETY: both regions lie inside the usable range, are aligned for their types,
        // and the caller guarantees nothing live occupies them.
        unsafe {
            let entry = ErasedEntry::place(storage, entry);
            core.as_ptr().write(Self::new(entry));
        }
        Placement { core, frame_top }
    }

    fn new(entry: ErasedEntry) -> Self {
        Self {
            parent_sp: Cell::new(0),
            child_sp: Cell::new(0),
            command: Cell::new(Command::Resume(Resume::Continue)),
            outcome: Cell::new(None),
            entry: Cell::new(Some(entry)),
            cookie: NEXT_COOKIE.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// The process-unique token this fiber's root suppresses.
    pub(crate) fn cookie(&self) -> u64 {
        self.cookie
    }

    pub(crate) fn parent_sp(&self) -> usize {
        self.parent_sp.get()
    }

    pub(crate) fn parent_sp_slot(&self) -> *mut usize {
        self.parent_sp.as_ptr()
    }

    pub(crate) fn child_sp(&self) -> usize {
        self.child_sp.get()
    }

    pub(crate) fn child_sp_slot(&self) -> *mut usize {
        self.child_sp.as_ptr()
    }

    pub(crate) fn set_child_sp(&self, sp: usize) {
        self.child_sp.set(sp);
    }

    pub(crate) fn issue(&self, command: Command) {
        self.command.set(command);
    }

    pub(crate) fn command(&self) -> Command {
        self.command.get()
    }

    pub(crate) fn report(&self, outcome: Outcome) {
        self.outcome.set(Some(outcome));
    }

    pub(crate) fn take_outcome(&self) -> Outcome {
        self.outcome
            .take()
            .expect("the fiber returned control without reporting an outcome")
    }

    pub(crate) fn take_entry(&self) -> Option<ErasedEntry> {
        self.entry.take()
    }

    fn classify(&self, payload: Box<dyn Any + Send>) -> Outcome {
        match payload.downcast::<ForcedUnwind>() {
            Ok(token) if token.cookie == self.cookie => Outcome::Unwound,
            Ok(token) => Outcome::Panicked(token),
            Err(payload) => Outcome::Panicked(payload),
        }
    }
}

fn align_down(address: usize, alignment: usize) -> usize {
    address & !(alignment - 1)
}

/// Runs a fiber's entry on its own stack and never returns.
///
/// Every panic, including the forced-unwind token, is caught here on the fiber stack,
/// so unwinding never crosses the context switch and backtraces end at this frame.
pub(crate) extern "C" fn fiber_root(core: *const FiberCore) -> ! {
    // SAFETY: the owning execution keeps the stack, and so the block, alive until the
    // fiber is terminal.
    let core = unsafe { &*core };
    let outcome = match catch_unwind(AssertUnwindSafe(|| {
        if let Some(entry) = core.take_entry() {
            entry.call();
        }
    })) {
        Ok(()) => Outcome::Complete,
        Err(payload) => core.classify(payload),
    };
    core.report(outcome);
    // SAFETY: the carrier saved its context in the parent slot before switching here.
    unsafe { arch::context_finish(core.parent_sp()) }
}

#[cfg(test)]
#[path = "context_test.rs"]
mod context_test;
