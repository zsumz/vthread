//! Type-erased ownership of a fiber's entry closure inside the fiber's own stack.

use std::{
    mem::ManuallyDrop,
    ptr::{self, NonNull},
};

/// An `FnOnce()` moved into caller-provided storage with its type forgotten.
///
/// The storage is the top of the fiber's stack, so no heap is involved. The closure's
/// lifetime is erased too; the owning execution guarantees the entry runs or is dropped
/// before anything it borrows expires.
pub(crate) struct ErasedEntry {
    data: NonNull<()>,
    call: unsafe fn(NonNull<()>),
    drop: unsafe fn(NonNull<()>),
}

impl ErasedEntry {
    /// Moves `entry` into `storage`.
    ///
    /// # Safety
    ///
    /// `storage` must be valid for writes of `F`, aligned for `F`, and stay untouched
    /// until this handle is called or dropped.
    pub(crate) unsafe fn place<F: FnOnce()>(storage: NonNull<()>, entry: F) -> Self {
        // SAFETY: the caller guarantees the storage is valid and aligned for `F`.
        unsafe { storage.cast::<F>().write(entry) };
        Self {
            data: storage,
            call: call_placed::<F>,
            drop: drop_placed::<F>,
        }
    }

    /// Moves the entry out of its storage and runs it exactly once.
    pub(crate) fn call(self) {
        let entry = ManuallyDrop::new(self);
        // SAFETY: `data` was written by `place` for the closure type behind `call`, and
        // the ManuallyDrop wrapper guarantees the drop function never runs afterwards.
        unsafe { (entry.call)(entry.data) }
    }
}

impl Drop for ErasedEntry {
    fn drop(&mut self) {
        // SAFETY: the storage still holds the closure because `call` consumes handles
        // without running Drop.
        unsafe { (self.drop)(self.data) }
    }
}

unsafe fn call_placed<F: FnOnce()>(data: NonNull<()>) {
    // SAFETY: the caller passes the storage `place` filled with an `F` exactly once.
    let entry: F = unsafe { data.cast::<F>().read() };
    entry();
}

unsafe fn drop_placed<F>(data: NonNull<()>) {
    // SAFETY: the caller passes the storage `place` filled with an `F` exactly once.
    unsafe { ptr::drop_in_place(data.cast::<F>().as_ptr()) }
}

#[cfg(test)]
#[path = "entry_test.rs"]
mod entry_test;
