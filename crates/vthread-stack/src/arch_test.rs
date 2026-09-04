#![cfg(all(target_arch = "aarch64", target_os = "macos"))]

use std::ptr::NonNull;

use super::{aarch64_macos::FRAME_LEN, init_frame};
use crate::{MappedStack, STACK_ALIGNMENT, context::FiberCore, entry::ErasedEntry};

#[test]
fn the_saved_context_keeps_the_stack_pointer_aligned() {
    assert_eq!(FRAME_LEN % STACK_ALIGNMENT, 0);
}

#[test]
fn the_first_frame_sits_just_below_the_stack_base() {
    let stack = MappedStack::new(64 * 1024, 0).expect("allocate stack");
    let core = FiberCore::new(ErasedEntry::new(|| {}));
    // SAFETY: the mapping is live and far larger than one frame; the core outlives it.
    let sp = unsafe { init_frame(&stack, NonNull::from(&core)) };
    assert_eq!(sp % STACK_ALIGNMENT, 0);
    assert_eq!(stack.base().get() - sp, FRAME_LEN);
    assert!(sp > stack.limit().get() + stack.guard_len());
}
