use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::{ContextKey, Fiber, FiberState, MappedStack, Resume, Suspension, suspend};

static TEXT: ContextKey<String> = ContextKey::new();

fn fiber(body: impl FnOnce() + 'static) -> Fiber {
    Fiber::new(MappedStack::new(128 * 1024, 0).unwrap(), body)
}

#[test]
fn a_context_reference_cannot_survive_its_resume_by_suspending() {
    let value = String::from("mounted context");
    let mut fiber = fiber(|| {
        TEXT.with(|text| {
            assert!(suspend(Suspension::YieldNow).is_err());
            assert_eq!(text, "mounted context");
        })
        .unwrap();
    });
    assert_eq!(
        fiber.resume_with_context(Resume::Continue, &TEXT, &value),
        FiberState::Complete,
        "the callback must finish before its borrowed context can be released"
    );
}

#[test]
fn callback_unwind_restores_the_running_fibers_suspension_context() {
    let value = String::from("mounted context");
    let mut fiber = fiber(|| {
        let result = catch_unwind(AssertUnwindSafe(|| {
            TEXT.with(|_| panic!("context callback failed"));
        }));
        assert!(result.is_err());
        suspend(Suspension::YieldNow).unwrap();
    });
    assert_eq!(
        fiber.resume_with_context(Resume::Continue, &TEXT, &value),
        FiberState::Suspended(Suspension::YieldNow)
    );
    assert_eq!(fiber.resume(), FiberState::Complete);
}

#[test]
fn nested_fibers_can_suspend_without_releasing_an_outer_context_borrow() {
    let value = String::from("outer context");
    let mut outer = fiber(|| {
        TEXT.with(|text| {
            let mut inner = fiber(|| {
                suspend(Suspension::YieldNow).unwrap();
            });
            assert_eq!(inner.resume(), FiberState::Suspended(Suspension::YieldNow));
            assert_eq!(inner.resume(), FiberState::Complete);
            assert_eq!(text, "outer context");
            assert!(suspend(Suspension::YieldNow).is_err());
        })
        .unwrap();
        suspend(Suspension::YieldNow).unwrap();
    });
    assert_eq!(
        outer.resume_with_context(Resume::Continue, &TEXT, &value),
        FiberState::Suspended(Suspension::YieldNow)
    );
    assert_eq!(outer.resume(), FiberState::Complete);
}

#[test]
fn nested_context_callbacks_restore_the_outer_borrow_restriction() {
    let value = String::from("nested context");
    let mut fiber = fiber(|| {
        TEXT.with(|outer| {
            TEXT.with(|inner| assert_eq!(outer, inner)).unwrap();
            assert!(suspend(Suspension::YieldNow).is_err());
        })
        .unwrap();
    });
    assert_eq!(
        fiber.resume_with_context(Resume::Continue, &TEXT, &value),
        FiberState::Complete
    );
}
