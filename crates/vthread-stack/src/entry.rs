//! Type-erased ownership of a fiber's entry closure until its first run.

use std::{mem::ManuallyDrop, ptr::NonNull};

/// A boxed `FnOnce()` whose type is forgotten so one fiber core can hold any entry.
///
/// The closure's lifetime is erased too; the owning execution guarantees the entry
/// runs or is dropped before anything it borrows expires.
pub(crate) struct ErasedEntry {
    data: NonNull<()>,
    call: unsafe fn(NonNull<()>),
    drop: unsafe fn(NonNull<()>),
}

impl ErasedEntry {
    pub(crate) fn new<F: FnOnce()>(entry: F) -> Self {
        let data = NonNull::from(Box::leak(Box::new(entry))).cast::<()>();
        Self {
            data,
            call: call_boxed::<F>,
            drop: drop_boxed::<F>,
        }
    }

    /// Runs the entry exactly once, consuming this handle.
    pub(crate) fn call(self) {
        let entry = ManuallyDrop::new(self);
        // SAFETY: `data` came from `new` for the closure type behind `call`, and the
        // ManuallyDrop wrapper guarantees the drop function never runs for this handle.
        unsafe { (entry.call)(entry.data) }
    }
}

impl Drop for ErasedEntry {
    fn drop(&mut self) {
        // SAFETY: this handle still owns the box because `call` consumes handles
        // without running Drop.
        unsafe { (self.drop)(self.data) }
    }
}

unsafe fn call_boxed<F: FnOnce()>(data: NonNull<()>) {
    // SAFETY: the caller passes the pointer `new` leaked from a `Box<F>` exactly once.
    let entry: F = *unsafe { Box::from_raw(data.cast::<F>().as_ptr()) };
    entry();
}

unsafe fn drop_boxed<F>(data: NonNull<()>) {
    // SAFETY: the caller passes the pointer `new` leaked from a `Box<F>` exactly once.
    drop(unsafe { Box::from_raw(data.cast::<F>().as_ptr()) });
}

#[cfg(test)]
#[path = "entry_test.rs"]
mod entry_test;
