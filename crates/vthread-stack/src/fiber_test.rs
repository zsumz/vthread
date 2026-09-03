use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    time::{Duration, Instant},
};

use corosensei::stack::DefaultStack;

use super::{Fiber, FiberState, ParkRequest, ParkToken, Resume, Suspension, suspend};

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

    assert_eq!(fiber.resume(), FiberState::Suspended(Suspension::YieldNow));
    assert_eq!(&*trace.borrow(), &["before"]);
    assert_eq!(fiber.resume(), FiberState::Complete);
    assert_eq!(&*trace.borrow(), &["before", "after"]);
    assert!(fiber.is_complete());
    drop(fiber.into_stack());
}

#[test]
fn resume_decision_returns_to_the_suspension_point() {
    let observed = Rc::new(Cell::new(Resume::Continue));
    let body_observed = Rc::clone(&observed);
    let stack = DefaultStack::new(128 * 1024).expect("allocate stack");
    let mut fiber = Fiber::new(stack, move || {
        body_observed.set(suspend(Suspension::YieldNow).unwrap());
    });

    assert_eq!(fiber.resume(), FiberState::Suspended(Suspension::YieldNow));
    assert_eq!(fiber.resume_with(Resume::Interrupt), FiberState::Complete);
    assert_eq!(observed.get(), Resume::Interrupt);
    drop(fiber.into_stack());
}

#[test]
fn suspended_fiber_can_move_before_resuming() {
    let stack = DefaultStack::new(128 * 1024).expect("allocate stack");
    let mut fiber = Fiber::new(stack, || {
        nested_yield();
        nested_yield();
    });

    assert_eq!(fiber.resume(), FiberState::Suspended(Suspension::YieldNow));
    let mut moved = fiber;
    assert_eq!(moved.resume(), FiberState::Suspended(Suspension::YieldNow));
    assert_eq!(moved.resume(), FiberState::Complete);
    drop(moved.into_stack());
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
    assert_eq!(
        error.to_string(),
        "no virtual-thread stack is mounted on this carrier"
    );
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

#[test]
fn completed_resume_panics_before_installing_a_stale_yielder() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let mut fiber = Fiber::new(DefaultStack::new(128 * 1024).unwrap(), || {});
    assert_eq!(fiber.resume(), FiberState::Complete);
    let wrong_mount = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&wrong_mount);
    let owner = std::thread::current().id();
    let previous = Arc::new(std::panic::take_hook());
    let forwarded = Arc::clone(&previous);
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current().id() == owner {
            observed.store(has_mounted_yielder(), Ordering::Relaxed);
        }
        forwarded(info);
    }));
    let result = catch_unwind(AssertUnwindSafe(|| fiber.resume()));
    std::panic::set_hook(Box::new(move |info| previous(info)));
    assert!(result.is_err());
    assert!(!wrong_mount.load(Ordering::Relaxed));
}

#[test]
fn incomplete_stack_extraction_reclaims_with_its_yielder_mounted() {
    struct ObserveMount(Rc<Cell<bool>>);
    impl Drop for ObserveMount {
        fn drop(&mut self) {
            self.0.set(has_mounted_yielder());
        }
    }
    let mounted = Rc::new(Cell::new(false));
    let observed = Rc::clone(&mounted);
    let mut fiber = Fiber::new(DefaultStack::new(128 * 1024).unwrap(), move || {
        let _probe = ObserveMount(observed);
        suspend(Suspension::YieldNow).unwrap();
    });
    assert_eq!(fiber.resume(), FiberState::Suspended(Suspension::YieldNow));
    let result = catch_unwind(AssertUnwindSafe(|| fiber.into_stack()));
    assert!(result.is_err());
    assert!(mounted.get());
    assert!(!has_mounted_yielder());
}

fn has_mounted_yielder() -> bool {
    // The guard captures and then restores the current mount without dereferencing it.
    !super::MountGuard::install(std::ptr::null())
        .previous
        .is_null()
}
