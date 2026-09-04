#![cfg(all(target_arch = "aarch64", target_os = "macos"))]

use core::arch::asm;
use std::{cell::Cell, ptr, rc::Rc};

use super::fpcr;
use crate::{FiberState, MappedStack, Resume, Suspension, context::FiberCore, engine};

/// FPCR rounding-mode field: both bits set selects round toward zero.
const ROUND_TOWARD_ZERO: u64 = 0b11 << 22;

fn write_fpcr(value: u64) {
    // SAFETY: FPCR only steers floating-point rounding; nothing here depends on it.
    unsafe {
        asm!("msr fpcr, {}", in(reg) value, options(nomem, nostack));
    }
}

#[test]
fn floating_point_control_state_stays_with_its_context() {
    let carrier = fpcr();
    assert_eq!(
        carrier & ROUND_TOWARD_ZERO,
        0,
        "the test needs the default mode"
    );
    let handle = Rc::new(Cell::new(ptr::null::<FiberCore>()));
    let body_handle = Rc::clone(&handle);
    let stack = MappedStack::new(128 * 1024, 0).expect("allocate stack");
    let entry = move || {
        write_fpcr(fpcr() | ROUND_TOWARD_ZERO);
        // SAFETY: the handle names this execution's core on this carrier.
        unsafe { engine::suspend(body_handle.get(), Suspension::YieldNow) };
        assert_eq!(fpcr() & ROUND_TOWARD_ZERO, ROUND_TOWARD_ZERO);
    };
    // SAFETY: the entry borrows nothing.
    let mut execution = unsafe { engine::Execution::start(stack, entry) };
    handle.set(execution.core_ptr());

    assert_eq!(
        execution.resume(Resume::Continue),
        FiberState::Suspended(Suspension::YieldNow)
    );
    assert_eq!(
        fpcr(),
        carrier,
        "the fiber's rounding mode leaked to the carrier"
    );
    assert_eq!(execution.resume(Resume::Continue), FiberState::Complete);
    assert_eq!(fpcr(), carrier);
}
