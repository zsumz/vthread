//! Linux x86-64 context switch: System V callee-saved state and the floating-point
//! control words.
//!
//! A saved context is one 72-byte frame at the saved stack pointer:
//!
//! ```text
//! offset  contents
//!      0  mxcsr
//!      4  x87 control word
//!      8  padding
//!     16  r15
//!     24  r14
//!     32  r13
//!     40  r12
//!     48  rbx
//!     56  rbp
//!     64  return address
//! ```
//!
//! MXCSR and the x87 control word are restored only when the two contexts disagree,
//! because loading them costs far more than the comparison and the mode is almost
//! always shared.
//!
//! The saved stack pointer is congruent to 8 modulo 16, exactly as inside a called
//! function. A fabricated first frame carries the fiber core in rbx, a zero rbp, and
//! the bootstrap trampoline as its return address. The trampoline pushes a zero return
//! address and jumps to the fiber root, so frame-pointer walkers and DWARF unwinders
//! both stop at the root instead of wandering into the carrier stack.
//!
//! Items are gated one by one rather than the whole file so the module, and its
//! sibling test declaration, bind on every target the architecture lock analyzes.

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
use core::arch::{asm, naked_asm};
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
use std::ptr::{self, NonNull};

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
use crate::context::{FiberCore, fiber_root};

/// Bytes one saved context occupies on its stack.
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
pub(crate) const FRAME_LEN: usize = 72;
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
const SLOT_MXCSR: usize = 0;
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
const SLOT_X87_CONTROL: usize = 4;
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
const SLOT_RBX: usize = 48;
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
const SLOT_RETURN: usize = 64;

/// Saves the current context into a frame, stores that frame's address through
/// `save_current`, and resumes the context saved at `restore`.
///
/// # Safety
///
/// `restore` must be a frame written by this function, by `init_frame`, or by the
/// trampoline protocol, on a stack that is still mapped and not otherwise in use.
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn context_switch(save_current: *mut usize, restore: usize) {
    naked_asm!(
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "sub rsp, 16",
        "stmxcsr [rsp]",
        "fnstcw [rsp + 4]",
        "mov eax, [rsp]",
        "movzx ecx, word ptr [rsp + 4]",
        "mov [rdi], rsp",
        "mov rsp, rsi",
        // Loading the control words is slow; skip each one whose value already matches.
        "cmp eax, [rsp]",
        "je 2f",
        "ldmxcsr [rsp]",
        "2:",
        "cmp cx, [rsp + 4]",
        "je 3f",
        "fldcw [rsp + 4]",
        "3:",
        "add rsp, 16",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "ret",
    )
}

/// Abandons the current context for good and resumes the context saved at `restore`.
///
/// # Safety
///
/// Same as `context_switch`; additionally nothing may ever switch back to the caller.
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn context_finish(restore: usize) -> ! {
    naked_asm!(
        "sub rsp, 8",
        "stmxcsr [rsp]",
        "fnstcw [rsp + 4]",
        "mov eax, [rsp]",
        "movzx ecx, word ptr [rsp + 4]",
        "mov rsp, rdi",
        "cmp eax, [rsp]",
        "je 2f",
        "ldmxcsr [rsp]",
        "2:",
        "cmp cx, [rsp + 4]",
        "je 3f",
        "fldcw [rsp + 4]",
        "3:",
        "add rsp, 16",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "ret",
    )
}

/// First code a fiber runs: hands the core to the root with a terminated frame chain.
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[unsafe(naked)]
unsafe extern "C" fn trampoline() -> ! {
    naked_asm!(
        "mov rdi, rbx",
        "xor ebp, ebp",
        "push 0",
        "jmp {root}",
        "ud2",
        root = sym fiber_root,
    )
}

/// Writes a fiber's first saved context below `frame_top` and returns its stack pointer.
///
/// # Safety
///
/// `frame_top` must be aligned to sixteen bytes with at least `FRAME_LEN` mapped bytes
/// below it, and `core` must stay valid until the fiber is terminal.
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
pub(crate) unsafe fn init_frame(frame_top: usize, core: NonNull<FiberCore>) -> usize {
    let sp = frame_top - FRAME_LEN;
    let frame = sp as *mut u8;
    // SAFETY: the caller guarantees the frame below `frame_top` is mapped and unused.
    unsafe {
        ptr::write_bytes(frame, 0, FRAME_LEN);
        frame
            .add(SLOT_RBX)
            .cast::<usize>()
            .write(core.as_ptr() as usize);
        frame
            .add(SLOT_RETURN)
            .cast::<usize>()
            .write(trampoline as *const () as usize);
        frame.add(SLOT_MXCSR).cast::<u32>().write(mxcsr());
        frame
            .add(SLOT_X87_CONTROL)
            .cast::<u16>()
            .write(x87_control_word());
    }
    sp
}

/// Reads the SSE control and status register so a new fiber inherits the carrier's mode.
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
pub(crate) fn mxcsr() -> u32 {
    let mut value: u32 = 0;
    // SAFETY: stmxcsr writes exactly the four bytes of `value` and nothing else.
    unsafe {
        asm!("stmxcsr [{}]", in(reg) &raw mut value, options(nostack, preserves_flags));
    }
    value
}

/// Reads the x87 control word so a new fiber inherits the carrier's mode.
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
pub(crate) fn x87_control_word() -> u16 {
    let mut value: u16 = 0;
    // SAFETY: fnstcw writes exactly the two bytes of `value` and nothing else.
    unsafe {
        asm!("fnstcw [{}]", in(reg) &raw mut value, options(nostack, preserves_flags));
    }
    value
}

#[cfg(test)]
#[path = "x86_64_sysv_test.rs"]
mod x86_64_sysv_test;
