use std::ptr;

use super::Execution;
use crate::{FiberState, MappedStack, Resume};

#[test]
fn the_yielder_is_unknown_until_the_first_suspension() {
    let stack = MappedStack::new(128 * 1024, 0).expect("allocate stack");
    // SAFETY: the entry borrows nothing.
    let execution = unsafe { Execution::start(stack, || {}) };
    assert_eq!(execution.yielder(), ptr::null());
    assert!(!execution.is_complete());
}

#[test]
fn a_completed_execution_forgets_its_yielder() {
    let stack = MappedStack::new(128 * 1024, 0).expect("allocate stack");
    // SAFETY: the entry borrows nothing.
    let mut execution = unsafe { Execution::start(stack, || {}) };
    assert_eq!(execution.resume(Resume::Continue), FiberState::Complete);
    assert_eq!(execution.yielder(), ptr::null());
    assert!(execution.is_complete());
    drop(execution.into_stack());
}
