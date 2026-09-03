//! Anonymous private mappings for fiber stacks on the supported Unix targets.

use std::{
    io,
    ptr::{self, NonNull},
};

/// Returns the kernel page size, which both supported targets report as a power of two.
pub(crate) fn page_size() -> io::Result<usize> {
    // SAFETY: sysconf reads process configuration and has no memory preconditions.
    let reported = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    match usize::try_from(reported) {
        Ok(size) if size.is_power_of_two() => Ok(size),
        _ => Err(io::Error::other("kernel reported an unusable page size")),
    }
}

/// Reserves `len` bytes of inaccessible, private, anonymous address space.
pub(crate) fn reserve(len: usize) -> io::Result<NonNull<u8>> {
    // SAFETY: a PROT_NONE anonymous mapping at a kernel-chosen address touches no
    // existing memory and only becomes accessible through a later `enable` call.
    let mapping = unsafe {
        libc::mmap(
            ptr::null_mut(),
            len,
            libc::PROT_NONE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    if mapping == libc::MAP_FAILED {
        return Err(io::Error::last_os_error());
    }
    NonNull::new(mapping.cast::<u8>())
        .ok_or_else(|| io::Error::other("mmap returned a null mapping"))
}

/// Makes `len` bytes from `start` readable and writable.
///
/// # Safety
///
/// The range must be page aligned and lie inside one live reservation from [`reserve`].
pub(crate) unsafe fn enable(start: NonNull<u8>, len: usize) -> io::Result<()> {
    // SAFETY: the caller guarantees the range lies inside a live reservation.
    let result = unsafe {
        libc::mprotect(
            start.as_ptr().cast(),
            len,
            libc::PROT_READ | libc::PROT_WRITE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Unmaps one whole reservation.
///
/// # Safety
///
/// `start` and `len` must describe exactly one live reservation from [`reserve`] that no
/// stack frame, pointer, or other owner still uses.
pub(crate) unsafe fn release(start: NonNull<u8>, len: usize) -> io::Result<()> {
    // SAFETY: the caller guarantees exclusive ownership of the whole live mapping.
    let result = unsafe { libc::munmap(start.as_ptr().cast(), len) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(test)]
#[path = "stack_unix_test.rs"]
mod stack_unix_test;
