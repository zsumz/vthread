use std::{
    cell::{Cell, RefCell},
    hint::black_box,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    ptr,
    rc::Rc,
};

use super::{Execution, suspend};
use crate::{
    FiberState, MappedStack, Resume, Suspension,
    context::{FiberCore, ForcedUnwind},
};

type Handle = Rc<Cell<*const FiberCore>>;

struct Count(Rc<Cell<u32>>);

impl Drop for Count {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

fn start(entry: impl FnOnce(&Handle) + 'static) -> (Execution, Handle) {
    let handle: Handle = Rc::new(Cell::new(ptr::null()));
    let body_handle = Rc::clone(&handle);
    let stack = MappedStack::new(128 * 1024, 0).expect("allocate stack");
    // SAFETY: the entry borrows nothing.
    let execution = unsafe { Execution::start(stack, move || entry(&body_handle)) };
    handle.set(execution.yielder());
    (execution, handle)
}

fn park(handle: &Handle) -> Resume {
    // SAFETY: the handle names the execution whose entry is running on this carrier.
    unsafe { suspend(handle.get(), Suspension::YieldNow) }
}

#[test]
fn suspends_resumes_and_completes_in_order() {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let body_trace = Rc::clone(&trace);
    let (mut execution, _) = start(move |handle| {
        body_trace.borrow_mut().push("before");
        assert_eq!(park(handle), Resume::Continue);
        body_trace.borrow_mut().push("between");
        assert_eq!(park(handle), Resume::Interrupt);
        body_trace.borrow_mut().push("after");
    });
    assert!(!execution.is_complete());
    assert_eq!(
        execution.resume(Resume::Continue),
        FiberState::Suspended(Suspension::YieldNow)
    );
    assert_eq!(&*trace.borrow(), &["before"]);
    assert_eq!(
        execution.resume(Resume::Continue),
        FiberState::Suspended(Suspension::YieldNow)
    );
    assert_eq!(execution.resume(Resume::Interrupt), FiberState::Complete);
    assert_eq!(&*trace.borrow(), &["before", "between", "after"]);
    assert!(execution.is_complete());
    drop(execution.into_stack());
}

#[test]
fn a_never_started_execution_drops_its_entry_once_on_the_carrier() {
    let drops = Rc::new(Cell::new(0));
    let captured = Count(Rc::clone(&drops));
    let (execution, _) = start(move |_| drop(captured));
    drop(execution);
    assert_eq!(drops.get(), 1);
}

#[test]
fn a_panic_unwinds_the_fiber_stack_before_reaching_the_carrier() {
    let drops = Rc::new(Cell::new(0));
    let flag = Count(Rc::clone(&drops));
    let (mut execution, _) = start(move |handle| {
        let _flag = flag;
        park(handle);
        panic!("task panic");
    });
    execution.resume(Resume::Continue);
    let result = catch_unwind(AssertUnwindSafe(|| execution.resume(Resume::Continue)));
    assert!(result.is_err());
    assert_eq!(drops.get(), 1);
    assert!(execution.is_complete());
    drop(execution.into_stack());
}

#[test]
fn dropping_a_suspended_execution_runs_every_live_destructor() {
    let drops = Rc::new(Cell::new(0));
    let outer = Count(Rc::clone(&drops));
    let inner = Count(Rc::clone(&drops));
    let (mut execution, _) = start(move |handle| {
        let _outer = outer;
        nested(handle, inner);
    });
    fn nested(handle: &Handle, inner: Count) {
        let _inner = inner;
        park(handle);
    }
    execution.resume(Resume::Continue);
    assert!(catch_unwind(AssertUnwindSafe(|| drop(execution))).is_ok());
    assert_eq!(drops.get(), 2);
}

#[test]
fn a_caught_forced_unwind_is_reinjected_at_the_next_suspension() {
    let drops = Rc::new(Cell::new(0));
    let attempts = Rc::new(Cell::new(0));
    let guard = Count(Rc::clone(&drops));
    let body_attempts = Rc::clone(&attempts);
    let (mut execution, _) = start(move |handle| {
        let _guard = guard;
        loop {
            body_attempts.set(body_attempts.get() + 1);
            // Swallow whatever unwinds through the park and park again.
            let _ = catch_unwind(AssertUnwindSafe(|| park(handle)));
            if body_attempts.get() == 3 {
                park(handle);
            }
        }
    });
    execution.resume(Resume::Continue);
    drop(execution);
    assert_eq!(drops.get(), 1);
    assert_eq!(attempts.get(), 3);
}

#[test]
fn a_foreign_forced_unwind_token_is_an_ordinary_panic() {
    let (mut execution, _) = start(|_| resume_unwind(Box::new(ForcedUnwind::new(0))));
    let payload = catch_unwind(AssertUnwindSafe(|| execution.resume(Resume::Continue)))
        .expect_err("a foreign token must not be suppressed");
    assert!(payload.is::<ForcedUnwind>());
    assert!(execution.is_complete());
}

#[test]
fn extracting_a_suspended_stack_fails_and_still_reclaims_it() {
    let drops = Rc::new(Cell::new(0));
    let guard = Count(Rc::clone(&drops));
    let (mut execution, _) = start(move |handle| {
        let _guard = guard;
        park(handle);
    });
    execution.resume(Resume::Continue);
    assert!(catch_unwind(AssertUnwindSafe(|| execution.into_stack())).is_err());
    assert_eq!(drops.get(), 1);
}

fn mix(rounds: u64) -> (u64, u64, u64, u64) {
    let (mut a, mut b, mut c, mut d) = (1u64, 2u64, 3u64, 5u64);
    for round in 0..rounds {
        a = a.rotate_left(1) ^ round;
        b = b.wrapping_mul(31).wrapping_add(a);
        c ^= b.rotate_right(7);
        d = d.wrapping_add(c);
    }
    (a, b, c, d)
}

#[test]
fn callee_saved_state_survives_a_million_switches() {
    const ROUNDS: u64 = 500_000;
    let observed = Rc::new(Cell::new((0, 0, 0, 0)));
    let body_observed = Rc::clone(&observed);
    let (mut execution, _) = start(move |handle| {
        let (mut a, mut b, mut c, mut d) = (1u64, 2u64, 3u64, 5u64);
        for round in 0..ROUNDS {
            a = black_box(a).rotate_left(1) ^ round;
            b = black_box(b).wrapping_mul(31).wrapping_add(a);
            c ^= black_box(b).rotate_right(7);
            d = black_box(d).wrapping_add(c);
            park(handle);
        }
        body_observed.set((a, b, c, d));
    });
    let (mut a, mut b, mut c, mut d) = (1u64, 2u64, 3u64, 5u64);
    for round in 0..ROUNDS {
        assert_eq!(
            execution.resume(Resume::Continue),
            FiberState::Suspended(Suspension::YieldNow)
        );
        a = black_box(a).rotate_left(1) ^ round;
        b = black_box(b).wrapping_mul(31).wrapping_add(a);
        c ^= black_box(b).rotate_right(7);
        d = black_box(d).wrapping_add(c);
    }
    assert_eq!(execution.resume(Resume::Continue), FiberState::Complete);
    let expected = mix(ROUNDS);
    assert_eq!((a, b, c, d), expected, "carrier registers were corrupted");
    assert_eq!(observed.get(), expected, "fiber registers were corrupted");
}

#[test]
fn deep_recursion_keeps_the_stack_pointer_aligned() {
    fn descend(handle: &Handle, depth: u32) -> usize {
        if depth == 0 {
            park(handle);
            return format!("{}", black_box(depth)).len();
        }
        descend(handle, depth - 1) + format!("{depth}").len()
    }
    let total = Rc::new(Cell::new(0));
    let body_total = Rc::clone(&total);
    let (mut execution, _) = start(move |handle| body_total.set(descend(handle, 300)));
    assert_eq!(
        execution.resume(Resume::Continue),
        FiberState::Suspended(Suspension::YieldNow)
    );
    assert_eq!(execution.resume(Resume::Continue), FiberState::Complete);
    let expected: usize = (0..=300u32).map(|depth| depth.to_string().len()).sum();
    assert_eq!(total.get(), expected);
}
