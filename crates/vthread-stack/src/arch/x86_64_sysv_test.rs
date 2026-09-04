#![cfg(all(target_arch = "x86_64", target_os = "linux"))]

use core::arch::asm;
use std::{cell::Cell, ptr, rc::Rc};

use super::{mxcsr, x87_control_word};
use crate::{FiberState, MappedStack, Resume, Suspension, context::FiberCore, engine};

/// MXCSR rounding-control field: both bits set selects round toward zero.
const ROUND_TOWARD_ZERO: u32 = 0b11 << 13;

fn write_mxcsr(value: u32) {
    // SAFETY: MXCSR only steers SSE rounding and exception masking; the value came
    // from stmxcsr with rounding bits added, so no reserved bit is set.
    unsafe {
        asm!("ldmxcsr [{}]", in(reg) &raw const value, options(nostack, preserves_flags));
    }
}

#[test]
fn the_default_control_words_are_the_abi_defaults() {
    assert_eq!(mxcsr() & ROUND_TOWARD_ZERO, 0);
    assert_eq!(
        x87_control_word() & 0x0C00,
        0,
        "x87 rounds to nearest by default"
    );
}

#[test]
fn floating_point_control_state_stays_with_its_context() {
    let carrier = mxcsr();
    assert_eq!(
        carrier & ROUND_TOWARD_ZERO,
        0,
        "the test needs the default mode"
    );
    let handle = Rc::new(Cell::new(ptr::null::<FiberCore>()));
    let body_handle = Rc::clone(&handle);
    let stack = MappedStack::new(128 * 1024, 0).expect("allocate stack");
    let entry = move || {
        write_mxcsr(mxcsr() | ROUND_TOWARD_ZERO);
        // SAFETY: the handle names this execution's core on this carrier.
        unsafe { engine::suspend(body_handle.get(), Suspension::YieldNow) };
        assert_eq!(mxcsr() & ROUND_TOWARD_ZERO, ROUND_TOWARD_ZERO);
    };
    // SAFETY: the entry borrows nothing.
    let mut execution = unsafe { engine::Execution::start(stack, entry) };
    handle.set(execution.core_ptr());

    assert_eq!(
        execution.resume(Resume::Continue),
        FiberState::Suspended(Suspension::YieldNow)
    );
    assert_eq!(
        mxcsr(),
        carrier,
        "the fiber's rounding mode leaked to the carrier"
    );
    assert_eq!(execution.resume(Resume::Continue), FiberState::Complete);
    assert_eq!(mxcsr(), carrier);
}
