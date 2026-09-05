//! Terminal transfer must return to the suspended caller with the shared outcome.

use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use crate::{Fiber, FiberState, MappedStack, ParkRequest, ParkToken, Resume, Suspension};

struct CountDrop(Rc<Cell<usize>>);

impl Drop for CountDrop {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn immediate_completion_returns_to_the_caller_and_reuses_the_same_stack() {
    let mut stack = MappedStack::new(128 * 1024, 0).unwrap();
    let address = stack.limit();
    let entries = Rc::new(Cell::new(0));
    let drops = Rc::new(Cell::new(0));
    for generation in 1..=256 {
        let observed = Rc::clone(&entries);
        let guard = CountDrop(Rc::clone(&drops));
        let mut fiber = Fiber::new(stack, move || {
            let _guard = guard;
            observed.set(generation);
        });
        assert_eq!(fiber.resume(), FiberState::Complete);
        assert!(fiber.is_complete());
        assert_eq!(entries.get(), generation);
        assert_eq!(drops.get(), generation);
        stack = fiber.into_stack();
        assert_eq!(stack.limit(), address);
    }
}

#[test]
fn completion_after_yield_and_park_returns_the_shared_outcome() {
    let request = ParkRequest::new(ParkToken::new(7, 11), None);
    let expected = request.clone();
    let drops = Rc::new(Cell::new(0));
    let guard = CountDrop(Rc::clone(&drops));
    let mut fiber = Fiber::new(MappedStack::new(128 * 1024, 0).unwrap(), move || {
        let _guard = guard;
        assert_eq!(crate::suspend(Suspension::YieldNow), Ok(Resume::Continue));
        assert_eq!(
            crate::suspend(Suspension::Park(request)),
            Ok(Resume::Interrupt)
        );
    });
    assert_eq!(fiber.resume(), FiberState::Suspended(Suspension::YieldNow));
    assert_eq!(
        fiber.resume(),
        FiberState::Suspended(Suspension::Park(expected))
    );
    assert_eq!(drops.get(), 0);
    assert_eq!(fiber.resume_with(Resume::Interrupt), FiberState::Complete);
    assert_eq!(drops.get(), 1);
}

#[test]
fn panic_reports_its_payload_and_allows_terminal_stack_reuse() {
    let mut stack = MappedStack::new(128 * 1024, 0).unwrap();
    let address = stack.limit();
    let drops = Rc::new(Cell::new(0));
    for generation in 1..=32 {
        let guard = CountDrop(Rc::clone(&drops));
        let mut fiber = Fiber::new(stack, move || {
            let _guard = guard;
            panic!("terminal transfer panic");
        });
        let payload = catch_unwind(AssertUnwindSafe(|| fiber.resume())).unwrap_err();
        assert_eq!(
            payload.downcast_ref::<&str>(),
            Some(&"terminal transfer panic")
        );
        assert!(fiber.is_complete());
        assert_eq!(drops.get(), generation);
        stack = fiber.into_stack();
        assert_eq!(stack.limit(), address);
    }
    let mut fiber = Fiber::new(stack, || {});
    assert_eq!(fiber.resume(), FiberState::Complete);
}

#[test]
fn forced_unwind_returns_after_dropping_nested_parked_frames() {
    fn nested(guard: CountDrop) {
        let _guard = guard;
        let request = ParkRequest::new(ParkToken::new(13, 17), None);
        crate::suspend(Suspension::Park(request)).unwrap();
        panic!("forced unwind must not return to the parked body");
    }
    let drops = Rc::new(Cell::new(0));
    let outer = CountDrop(Rc::clone(&drops));
    let inner = CountDrop(Rc::clone(&drops));
    let mut fiber = Fiber::new(MappedStack::new(128 * 1024, 0).unwrap(), move || {
        let _outer = outer;
        nested(inner);
    });
    assert!(matches!(
        fiber.resume(),
        FiberState::Suspended(Suspension::Park(_))
    ));
    assert_eq!(drops.get(), 0);
    assert!(catch_unwind(AssertUnwindSafe(|| drop(fiber))).is_ok());
    assert_eq!(drops.get(), 2);
}
