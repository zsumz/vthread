use std::ptr::NonNull;

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
use super::aarch64_macos::FRAME_LEN;
use super::init_frame;
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
use super::x86_64_sysv::FRAME_LEN;
use crate::{MappedStack, STACK_ALIGNMENT, context::FiberCore, entry::ErasedEntry};

#[test]
fn the_first_frame_sits_just_below_the_stack_base() {
    let stack = MappedStack::new(64 * 1024, 0).expect("allocate stack");
    let core = FiberCore::new(ErasedEntry::new(|| {}));
    // SAFETY: the mapping is live and far larger than one frame; the core outlives it.
    let sp = unsafe { init_frame(&stack, NonNull::from(&core)) };
    assert_eq!(stack.base().get() - sp, FRAME_LEN);
    assert_eq!((sp + FRAME_LEN) % STACK_ALIGNMENT, 0);
    assert!(sp > stack.limit().get() + stack.guard_len());
    assert!(
        FRAME_LEN <= stack.guard_len(),
        "one page must always hold the first frame"
    );
}
