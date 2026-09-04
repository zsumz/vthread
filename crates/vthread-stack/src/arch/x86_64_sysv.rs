//! Linux x86-64 context switch: System V callee-saved state and the floating-point
//! control words.
//!
//! A saved context is one 32-byte frame at the saved stack pointer:
//!
//! ```text
//! offset  contents
//!      0  return address
//!      8  mxcsr
//!     12  x87 control word
//!     16  rbx
//!     24  rbp
//! ```
//!
//! The inline switch declares r12-r15 clobbered, letting the compiler preserve only
//! values that are actually live. RBX and RBP are LLVM-reserved and stay in the frame.
//! A resume uses `call` and a suspension uses the matching `ret`, keeping the CPU's
//! return predictor paired across the stack switch.
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
pub(crate) const FRAME_LEN: usize = 32;
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
const SLOT_RETURN: usize = 0;
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
const SLOT_MXCSR: usize = 8;
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
const SLOT_X87_CONTROL: usize = 12;
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
const SLOT_RBX: usize = 16;

/// Resumes a child context through a call paired with the child's eventual return.
///
/// # Safety
///
/// `restore` must be a child frame written by [`context_suspend`] or [`init_frame`]
/// on a stack that is still mapped and not otherwise in use.
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[inline(always)]
pub(crate) unsafe fn context_resume(
    save_current: *mut usize,
    restore: usize,
    transfer: usize,
) -> usize {
    let returned;
    // SAFETY: the caller owns both contexts; the assembly restores this stack before
    // falling through and declares every register the System V ABI permits it to alter.
    unsafe {
        asm!(
        "push rbp",
        "push rbx",
        "sub rsp, 8",
        "stmxcsr [rsp]",
        "fnstcw [rsp + 4]",
        // The child target saves this stack pointer, installs its own, and eventually
        // returns here. Pairing CALL/RET keeps the return predictor on its fast path.
        "call qword ptr [rsi]",
        "add rsp, 8",
        "pop rbx",
        "pop rbp",
            in("rdi") save_current,
            in("rsi") restore,
            inlateout("rdx") transfer => returned,
        lateout("r12") _,
        lateout("r13") _,
        lateout("r14") _,
        lateout("r15") _,
        clobber_abi("sysv64"),
        );
    }
    returned
}

/// Suspends a child context and returns through its parent's pending resume call.
///
/// # Safety
///
/// `restore` must be the parent frame most recently saved through `save_current` by
/// [`context_resume`], and both frames must remain mapped and exclusively owned.
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[inline(always)]
pub(crate) unsafe fn context_suspend(
    save_current: *mut usize,
    restore: usize,
    transfer: usize,
) -> usize {
    let returned;
    // SAFETY: the caller owns both contexts; the resumed half restores this stack before
    // falling through and all ABI-visible clobbers are declared.
    unsafe {
        asm!(
        "push rbp",
        "push rbx",
        "sub rsp, 8",
        "stmxcsr [rsp]",
        "fnstcw [rsp + 4]",
        "lea rax, [rip + 4f]",
        "push rax",
        "mov eax, [rsp + 8]",
        "movzx ecx, word ptr [rsp + 12]",
        "mov [rdi], rsp",
        // Restore the parent's floating-point mode before returning to it.
        "cmp eax, [rsi + 8]",
        "je 2f",
        "ldmxcsr [rsi + 8]",
        "2:",
        "cmp cx, [rsi + 12]",
        "je 3f",
        "fldcw [rsi + 12]",
        "3:",
        "mov rsp, rsi",
        "ret",
        // A future resume calls this address while still on its new parent stack.
        "4:",
        "mov [rdi], rsp",
        "mov eax, [rsp + 8]",
        "movzx ecx, word ptr [rsp + 12]",
        "cmp eax, [rsi + 8]",
        "je 5f",
        "ldmxcsr [rsi + 8]",
        "5:",
        "cmp cx, [rsi + 12]",
        "je 6f",
        "fldcw [rsi + 12]",
        "6:",
        "lea rsp, [rsi + 16]",
        "pop rbx",
        "pop rbp",
            in("rdi") save_current,
            in("rsi") restore,
            inlateout("rdx") transfer => returned,
        lateout("r12") _,
        lateout("r13") _,
        lateout("r14") _,
        lateout("r15") _,
        clobber_abi("sysv64"),
        );
    }
    returned
}

/// Abandons the current context for good and resumes the context saved at `restore`.
///
/// # Safety
///
/// Same frame contract as [`context_suspend`]; additionally the caller is abandoned.
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn context_finish(restore: usize, transfer: usize) -> ! {
    naked_asm!(
        "sub rsp, 8",
        "stmxcsr [rsp]",
        "fnstcw [rsp + 4]",
        "mov eax, [rsp]",
        "movzx ecx, word ptr [rsp + 4]",
        "mov rsp, rdi",
        "cmp eax, [rdi + 8]",
        "je 2f",
        "ldmxcsr [rdi + 8]",
        "2:",
        "cmp cx, [rdi + 12]",
        "je 3f",
        "fldcw [rdi + 12]",
        "3:",
        "mov rdx, rsi",
        "mov rsp, rdi",
        "ret",
    )
}

/// First code a fiber runs: hands the core to the root with a terminated frame chain.
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[unsafe(naked)]
unsafe extern "C" fn trampoline() -> ! {
    naked_asm!(
        // The indirect resume call arrived on the parent stack. Save it, restore the
        // child's floating-point mode, and then install the fabricated child frame.
        "mov [rdi], rsp",
        "mov eax, [rsp + 8]",
        "movzx ecx, word ptr [rsp + 12]",
        "cmp eax, [rsi + 8]",
        "je 2f",
        "ldmxcsr [rsi + 8]",
        "2:",
        "cmp cx, [rsi + 12]",
        "je 3f",
        "fldcw [rsi + 12]",
        "3:",
        "lea rsp, [rsi + 16]",
        "pop rbx",
        "pop rbp",
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
