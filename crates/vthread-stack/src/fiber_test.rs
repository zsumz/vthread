use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    time::{Duration, Instant},
};

use corosensei::stack::DefaultStack;

use super::{Fiber, FiberState, ParkRequest, ParkToken, Suspension, suspend};

fn nested_yield() {
    suspend(Suspension::YieldNow).expect("fiber must be mounted");
}

#[test]
fn a_nested_function_can_suspend_and_resume() {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let body_trace = Rc::clone(&trace);
    let stack = DefaultStack::new(128 * 1024).expect("allocate stack");
    let mut fiber = Fiber::new(stack, move || {
        body_trace.borrow_mut().push("before");
        nested_yield();
        body_trace.borrow_mut().push("after");
    });

    assert_eq!(
        fiber.resume(),
        FiberState::Suspended(Suspension::YieldNow)
    );
    assert_eq!(&*trace.borrow(), &["before"]);
    assert_eq!(fiber.resume(), FiberState::Complete);
    assert_eq!(&*trace.borrow(), &["before", "after"]);
    assert!(fiber.is_complete());
    drop(fiber.into_stack());
}

#[test]
fn parking_requests_preserve_token_and_deadline() {
    let deadline = Instant::now() + Duration::from_secs(1);
    let request = ParkRequest::new(ParkToken::new(7, 3), Some(deadline));
    let stack = DefaultStack::new(128 * 1024).expect("allocate stack");
    let mut fiber = Fiber::new(stack, move || {
        suspend(Suspension::Park(request)).expect("fiber must be mounted");
    });

    match fiber.resume() {
        FiberState::Suspended(Suspension::Park(request)) => {
            assert_eq!(request.token().wait(), 7);
            assert_eq!(request.token().generation(), 3);
            assert_eq!(request.deadline(), Some(deadline));
        }
        state => panic!("unexpected fiber state: {state:?}"),
    }
    assert_eq!(fiber.resume(), FiberState::Complete);
    drop(fiber.into_stack());
}

#[test]
fn suspension_outside_a_fiber_is_rejected() {
    let error = suspend(Suspension::YieldNow).expect_err("no fiber is mounted");
    assert_eq!(error.to_string(), "no virtual-thread stack is mounted on this carrier");
}

#[test]
fn panic_unwinds_values_on_the_fiber_stack() {
    struct DropFlag(Rc<Cell<bool>>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    let dropped = Rc::new(Cell::new(false));
    let body_dropped = Rc::clone(&dropped);
    let stack = DefaultStack::new(128 * 1024).expect("allocate stack");
    let mut fiber = Fiber::new(stack, move || {
        let _flag = DropFlag(body_dropped);
        panic!("task panic");
    });

    let result = catch_unwind(AssertUnwindSafe(|| fiber.resume()));
    assert!(result.is_err());
    assert!(dropped.get());
    assert!(fiber.is_complete());
    drop(fiber.into_stack());
}
