//! Guard-page-backed fixed stacks stamped with the identity of their allocation.

use std::{io, num::NonZeroUsize, ptr::NonNull};

use crate::stack_unix;

/// Required stack pointer alignment at call boundaries on both supported targets.
pub const STACK_ALIGNMENT: usize = 16;

/// One private anonymous mapping whose lowest page is an inaccessible guard.
///
/// Stacks grow downward on both supported targets, so [`MappedStack::base`] is the
/// initial stack pointer and the guard page catches overflow before it can reach a
/// neighbouring mapping.
#[derive(Debug)]
pub struct MappedStack {
    mapping: NonNull<u8>,
    mapping_len: usize,
    guard_len: usize,
    base: NonZeroUsize,
    identity: u64,
}

// SAFETY: the mapping is plain process memory owned exclusively by this value. Only
// executing on it is carrier-affine, and that is enforced by the non-Send `Fiber`.
unsafe impl Send for MappedStack {}
// SAFETY: shared references expose only addresses and sizes, never the memory itself.
unsafe impl Sync for MappedStack {}

impl MappedStack {
    /// Maps at least `usable` bytes above one guard page and stamps the allocation.
    ///
    /// The usable capacity rounds up to whole pages. The identity is assigned by the
    /// allocating owner, normally a [`StackPool`](crate::StackPool), and never changes.
    pub fn new(usable: usize, identity: u64) -> io::Result<Self> {
        if usable == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "stack capacity must be positive",
            ));
        }
        let page = stack_unix::page_size()?;
        let usable_len = usable
            .checked_add(page - 1)
            .map(|rounded| rounded & !(page - 1))
            .ok_or_else(overflow)?;
        let mapping_len = usable_len.checked_add(page).ok_or_else(overflow)?;
        let mapping = stack_unix::reserve(mapping_len)?;
        // Owning the mapping from here on releases it on every later failure path.
        let mut stack = Self {
            mapping,
            mapping_len,
            guard_len: page,
            base: mapping.addr(),
            identity,
        };
        stack.base = mapping
            .addr()
            .get()
            .checked_add(mapping_len)
            .and_then(NonZeroUsize::new)
            .ok_or_else(overflow)?;
        // SAFETY: the range starts one guard page above the reservation and ends at its
        // top; both bounds are page aligned and inside the mapping `stack` now owns.
        unsafe { stack_unix::enable(stack.usable_start(), usable_len) }?;
        Ok(stack)
    }

    /// Highest usable address and initial stack pointer, aligned to [`STACK_ALIGNMENT`].
    pub fn base(&self) -> NonZeroUsize {
        self.base
    }

    /// Lowest address of the mapping, including the guard page.
    pub fn limit(&self) -> NonZeroUsize {
        self.mapping.addr()
    }

    /// Bytes of accessible stack above the guard page.
    pub fn usable_len(&self) -> usize {
        self.mapping_len - self.guard_len
    }

    /// Bytes in the inaccessible guard page.
    pub fn guard_len(&self) -> usize {
        self.guard_len
    }

    /// Identity stamped by the allocating owner.
    pub fn identity(&self) -> u64 {
        self.identity
    }

    fn usable_start(&self) -> NonNull<u8> {
        // SAFETY: the guard page lies inside the mapping, so the offset stays in bounds.
        unsafe { self.mapping.add(self.guard_len) }
    }
}

fn overflow() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "stack mapping size overflows the address space",
    )
}

impl Drop for MappedStack {
    fn drop(&mut self) {
        // SAFETY: this value exclusively owns the whole mapping and is being destroyed.
        let released = unsafe { stack_unix::release(self.mapping, self.mapping_len) };
        // A failed unmap cannot be handled or retried, and Drop must never unwind.
        drop(released);
    }
}

// Temporary bridge: corosensei keeps switching contexts on vthread-owned mappings until
// the native engine replaces it.
// SAFETY: the mapping has at least one usable page above a guard page and both bounds
// are page aligned, which satisfies the trait's alignment and minimum size requirements.
unsafe impl corosensei::stack::Stack for MappedStack {
    fn base(&self) -> corosensei::stack::StackPointer {
        self.base
    }

    fn limit(&self) -> corosensei::stack::StackPointer {
        self.limit()
    }
}

#[cfg(test)]
#[path = "stack_test.rs"]
mod stack_test;
