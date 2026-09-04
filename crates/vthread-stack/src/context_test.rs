use std::{
    cell::Cell,
    hint::black_box,
    mem,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use super::{Command, FiberCore, ForcedUnwind, Outcome, Placement};
use crate::{MappedStack, Resume, STACK_ALIGNMENT, Suspension, arch};

fn place(entry: impl FnOnce()) -> (MappedStack, Placement) {
    let stack = MappedStack::new(64 * 1024, 0).expect("allocate stack");
    // SAFETY: the mapping is fresh, so nothing is live on it.
    let placement = unsafe { FiberCore::place(&stack, entry) };
    (stack, placement)
}

fn core(placement: &Placement) -> &FiberCore {
    // SAFETY: every test keeps the stack that holds the block alive while using it.
    unsafe { placement.core.as_ref() }
}

#[test]
fn the_control_block_and_entry_sit_above_an_aligned_first_frame() {
    let captured = [7u8; 40];
    let (stack, placement) = place(move || {
        black_box(captured);
    });
    let block = placement.core.as_ptr() as usize;
    assert!(block + mem::size_of::<FiberCore>() <= stack.base().get());
    assert_eq!(block % mem::align_of::<FiberCore>(), 0);
    assert!(
        placement.frame_top + 40 <= block,
        "the entry storage sits between"
    );
    assert_eq!(placement.frame_top % STACK_ALIGNMENT, 0);
    assert!(placement.frame_top - arch::FRAME_LEN > stack.limit().get() + stack.guard_len());
}

#[test]
fn an_oversized_entry_is_rejected_before_anything_is_written() {
    let captured = [0u8; 70_000];
    let result = catch_unwind(AssertUnwindSafe(|| {
        place(move || {
            black_box(captured);
        })
    }));
    assert!(
        result.is_err(),
        "a 70000 byte entry cannot fit on a 64 KiB stack"
    );
}

#[test]
fn every_block_receives_a_distinct_nonzero_cookie() {
    let (_first_stack, first) = place(|| {});
    let (_second_stack, second) = place(|| {});
    assert_ne!(core(&first).cookie(), 0);
    assert_ne!(core(&first).cookie(), core(&second).cookie());
}

#[test]
fn commands_and_outcomes_round_trip_through_the_block() {
    let (_stack, placement) = place(|| {});
    let core = core(&placement);
    assert_eq!(core.command(), Command::Resume(Resume::Continue));
    core.issue(Command::ForceUnwind(core.cookie()));
    assert_eq!(core.command(), Command::ForceUnwind(core.cookie()));
    core.report(Outcome::Suspended(Suspension::YieldNow));
    assert!(matches!(
        core.take_outcome(),
        Outcome::Suspended(Suspension::YieldNow)
    ));
}

#[test]
fn only_the_matching_forced_unwind_token_is_suppressed() {
    let (_stack, placement) = place(|| {});
    let core = core(&placement);
    let own = Box::new(ForcedUnwind::new(core.cookie()));
    assert!(matches!(core.classify(own), Outcome::Unwound));
    let foreign = Box::new(ForcedUnwind::new(core.cookie() + 1));
    assert!(matches!(core.classify(foreign), Outcome::Panicked(_)));
    let user = Box::new("user panic");
    assert!(matches!(core.classify(user), Outcome::Panicked(_)));
}

#[test]
fn the_entry_is_taken_once_and_dropped_when_unused() {
    let drops = Rc::new(Cell::new(0));
    struct Counted(Rc<Cell<u32>>);
    impl Drop for Counted {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let captured = Counted(Rc::clone(&drops));
    let (_stack, placement) = place(move || drop(captured));
    let entry = core(&placement).take_entry();
    assert!(entry.is_some());
    assert!(core(&placement).take_entry().is_none());
    drop(entry);
    assert_eq!(drops.get(), 1);
}
