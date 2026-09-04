use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    time::{Duration, Instant},
};

use crate::{ContextKey, MappedStack};

use crate::suspend;
use crate::{Fiber, FiberState, ParkRequest, ParkToken, Resume, Suspension};

static TEST_CONTEXT: ContextKey<u64> = ContextKey::new();

fn nested_yield() {
    suspend(Suspension::YieldNow).expect("fiber must be mounted");
}

#[test]
fn a_nested_function_can_suspend_and_resume() {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let body_trace = Rc::clone(&trace);
    let stack = MappedStack::new(128 * 1024, 0).expect("allocate stack");
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
    let stack = MappedStack::new(128 * 1024, 0).expect("allocate stack");
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
    let stack = MappedStack::new(128 * 1024, 0).expect("allocate stack");
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
    let stack = MappedStack::new(128 * 1024, 0).expect("allocate stack");
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
fn typed_context_tracks_each_resume_and_restores_the_caller() {
    let observed = Rc::new(RefCell::new(Vec::new()));
    let body_observed = Rc::clone(&observed);
    let stack = MappedStack::new(128 * 1024, 0).expect("allocate stack");
    let mut fiber = Fiber::new(stack, move || {
        body_observed
            .borrow_mut()
            .push(TEST_CONTEXT.with(|value| *value));
        nested_yield();
        body_observed
            .borrow_mut()
            .push(TEST_CONTEXT.with(|value| *value));
    });

    assert!(TEST_CONTEXT.with(|_| ()).is_none());
    assert_eq!(
        fiber.resume_with_context(Resume::Continue, &TEST_CONTEXT, &7),
        FiberState::Suspended(Suspension::YieldNow)
    );
    assert!(TEST_CONTEXT.with(|_| ()).is_none());
    assert_eq!(
        fiber.resume_with_context(Resume::Continue, &TEST_CONTEXT, &11),
        FiberState::Complete
    );
    assert_eq!(&*observed.borrow(), &[Some(7), Some(11)]);
    assert!(TEST_CONTEXT.with(|_| ()).is_none());
    drop(fiber.into_stack());
}

#[test]
fn nested_fiber_context_restores_the_outer_value() {
    let inner_stack = MappedStack::new(128 * 1024, 0).expect("allocate inner stack");
    let outer_stack = MappedStack::new(128 * 1024, 0).expect("allocate outer stack");
    let mut outer = Fiber::new(outer_stack, move || {
        assert_eq!(TEST_CONTEXT.with(|value| *value), Some(7));
        let mut inner = Fiber::new(inner_stack, || {
            assert_eq!(TEST_CONTEXT.with(|value| *value), Some(11));
        });
        assert_eq!(
            inner.resume_with_context(Resume::Continue, &TEST_CONTEXT, &11),
            FiberState::Complete
        );
        assert_eq!(TEST_CONTEXT.with(|value| *value), Some(7));
        drop(inner.into_stack());
    });

    assert_eq!(
        outer.resume_with_context(Resume::Continue, &TEST_CONTEXT, &7),
        FiberState::Complete
    );
    assert!(TEST_CONTEXT.with(|_| ()).is_none());
    drop(outer.into_stack());
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
    let stack = MappedStack::new(128 * 1024, 0).expect("allocate stack");
    let mut fiber = Fiber::new(stack, move || {
        let _flag = DropFlag(body_dropped);
        panic!("task panic");
    });

    let result = catch_unwind(AssertUnwindSafe(|| {
        fiber.resume_with_context(Resume::Continue, &TEST_CONTEXT, &7)
    }));
    assert!(result.is_err());
    assert!(TEST_CONTEXT.with(|_| ()).is_none());
    assert!(dropped.get());
    assert!(fiber.is_complete());
    drop(fiber.into_stack());
}

#[test]
fn completed_resume_panics_before_installing_a_stale_mount() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let mut fiber = Fiber::new(MappedStack::new(128 * 1024, 0).unwrap(), || {});
    assert_eq!(fiber.resume(), FiberState::Complete);
    let wrong_mount = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&wrong_mount);
    let owner = std::thread::current().id();
    let previous = Arc::new(std::panic::take_hook());
    let forwarded = Arc::clone(&previous);
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current().id() == owner {
            observed.store(has_mounted_block(), Ordering::Relaxed);
        }
        forwarded(info);
    }));
    let result = catch_unwind(AssertUnwindSafe(|| fiber.resume()));
    std::panic::set_hook(Box::new(move |info| previous(info)));
    assert!(result.is_err());
    assert!(!wrong_mount.load(Ordering::Relaxed));
}

#[test]
fn incomplete_stack_extraction_reclaims_with_its_block_mounted() {
    struct ObserveMount(Rc<Cell<bool>>);
    impl Drop for ObserveMount {
        fn drop(&mut self) {
            self.0.set(has_mounted_block());
        }
    }
    let mounted = Rc::new(Cell::new(false));
    let observed = Rc::clone(&mounted);
    let mut fiber = Fiber::new(MappedStack::new(128 * 1024, 0).unwrap(), move || {
        let _probe = ObserveMount(observed);
        suspend(Suspension::YieldNow).unwrap();
    });
    assert_eq!(fiber.resume(), FiberState::Suspended(Suspension::YieldNow));
    let result = catch_unwind(AssertUnwindSafe(|| fiber.into_stack()));
    assert!(result.is_err());
    assert!(mounted.get());
    assert!(!has_mounted_block());
}

fn has_mounted_block() -> bool {
    !crate::mount::mounted_core().is_null()
}
