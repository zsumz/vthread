//! macOS ARM64 context switch: AAPCS64 callee-saved state, leaving Apple's x18 alone.
//!
//! A saved context is one 176-byte frame at the saved stack pointer:
//!
//! ```text
//! offset  contents
//!      0  x19, x20
//!     16  x21, x22
//!     32  x23, x24
//!     48  x25, x26
//!     64  x27, x28
//!     80  x29 (frame pointer), x30 (link register)
//!     96  d8, d9
//!    112  d10, d11
//!    128  d12, d13
//!    144  d14, d15
//!    160  fpcr, padding
//! ```
//!
//! FPCR is restored only when the two contexts disagree, because writing it costs far
//! more than the comparison and the mode is almost always shared.
//!
//! A fabricated first frame carries the fiber core in x19, a zero frame pointer, and
//! the bootstrap trampoline as its link register. The trampoline jumps to the fiber
//! root with a zero link register, so frame-pointer walkers and DWARF unwinders both
//! stop at the root instead of wandering into the carrier stack.
//!
//! Items are gated one by one rather than the whole file so the module, and its
//! sibling test declaration, bind on every target the architecture lock analyzes.
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
use core::arch::{asm, naked_asm};
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
use std::ptr::{self, NonNull};

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
use crate::context::{FiberCore, fiber_root};

/// Bytes one saved context occupies on its stack.
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
pub(crate) const FRAME_LEN: usize = 176;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const SLOT_X19: usize = 0;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const SLOT_X30: usize = 88;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const SLOT_FPCR: usize = 160;

/// Saves the current context into a frame, stores that frame's address through
/// `save_current`, and resumes the context saved at `restore`.
///
/// # Safety
///
/// `restore` must be a frame written by this function, by `init_frame`, or by the
/// trampoline protocol, on a stack that is still mapped and not otherwise in use.
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn context_switch(save_current: *mut usize, restore: usize) {
    naked_asm!(
        "sub sp, sp, #176",
        "stp x19, x20, [sp, #0]",
        "stp x21, x22, [sp, #16]",
        "stp x23, x24, [sp, #32]",
        "stp x25, x26, [sp, #48]",
        "stp x27, x28, [sp, #64]",
        "stp x29, x30, [sp, #80]",
        "stp d8, d9, [sp, #96]",
        "stp d10, d11, [sp, #112]",
        "stp d12, d13, [sp, #128]",
        "stp d14, d15, [sp, #144]",
        "mrs x2, fpcr",
        "str x2, [sp, #160]",
        "mov x3, sp",
        "str x3, [x0]",
        "mov sp, x1",
        // Writing FPCR is far costlier than reading it; skip it when the mode matches.
        "ldr x3, [sp, #160]",
        "cmp x2, x3",
        "b.eq 2f",
        "msr fpcr, x3",
        "2:",
        "ldp d14, d15, [sp, #144]",
        "ldp d12, d13, [sp, #128]",
        "ldp d10, d11, [sp, #112]",
        "ldp d8, d9, [sp, #96]",
        "ldp x29, x30, [sp, #80]",
        "ldp x27, x28, [sp, #64]",
        "ldp x25, x26, [sp, #48]",
        "ldp x23, x24, [sp, #32]",
        "ldp x21, x22, [sp, #16]",
        "ldp x19, x20, [sp, #0]",
        "add sp, sp, #176",
        "ret",
    )
}

/// Abandons the current context for good and resumes the context saved at `restore`.
///
/// # Safety
///
/// Same as `context_switch`; additionally nothing may ever switch back to the caller.
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn context_finish(restore: usize) -> ! {
    naked_asm!(
        "mrs x2, fpcr",
        "mov sp, x0",
        // Writing FPCR is far costlier than reading it; skip it when the mode matches.
        "ldr x3, [sp, #160]",
        "cmp x2, x3",
        "b.eq 2f",
        "msr fpcr, x3",
        "2:",
        "ldp d14, d15, [sp, #144]",
        "ldp d12, d13, [sp, #128]",
        "ldp d10, d11, [sp, #112]",
        "ldp d8, d9, [sp, #96]",
        "ldp x29, x30, [sp, #80]",
        "ldp x27, x28, [sp, #64]",
        "ldp x25, x26, [sp, #48]",
        "ldp x23, x24, [sp, #32]",
        "ldp x21, x22, [sp, #16]",
        "ldp x19, x20, [sp, #0]",
        "add sp, sp, #176",
        "ret",
    )
}

/// First code a fiber runs: hands the core to the root with a terminated frame chain.
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[unsafe(naked)]
unsafe extern "C" fn trampoline() -> ! {
    naked_asm!(
        "mov x0, x19",
        "mov x29, xzr",
        "mov x30, xzr",
        "b {root}",
        root = sym fiber_root,
    )
}

/// Writes a fiber's first saved context below `frame_top` and returns its stack pointer.
///
/// # Safety
///
/// `frame_top` must be aligned to sixteen bytes with at least `FRAME_LEN` mapped bytes
/// below it, and `core` must stay valid until the fiber is terminal.
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
pub(crate) unsafe fn init_frame(frame_top: usize, core: NonNull<FiberCore>) -> usize {
    let sp = frame_top - FRAME_LEN;
    let frame = sp as *mut u8;
    // SAFETY: the caller guarantees the frame below `frame_top` is mapped and unused.
    unsafe {
        ptr::write_bytes(frame, 0, FRAME_LEN);
        frame
            .add(SLOT_X19)
            .cast::<usize>()
            .write(core.as_ptr() as usize);
        frame
            .add(SLOT_X30)
            .cast::<usize>()
            .write(trampoline as *const () as usize);
        frame.add(SLOT_FPCR).cast::<u64>().write(fpcr());
    }
    sp
}

/// Reads the floating-point control register so a new fiber inherits the carrier's mode.
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
pub(crate) fn fpcr() -> u64 {
    let value: u64;
    // SAFETY: reading FPCR has no side effects and touches no memory.
    unsafe {
        asm!("mrs {}, fpcr", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

#[cfg(test)]
#[path = "aarch64_macos_test.rs"]
mod aarch64_macos_test;
