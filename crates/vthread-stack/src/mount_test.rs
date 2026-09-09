use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
    rc::Rc,
};

use super::{ContextKey, ContextSlot, CurrentMount, MountGuard};
use crate::{Fiber, FiberState, MappedStack, Resume, SuspendError, Suspension, suspend};

static NUMBER: ContextKey<u64> = ContextKey::new();
static OTHER_NUMBER: ContextKey<u64> = ContextKey::new();

#[test]
fn current_mount_is_two_machine_words() {
    assert_eq!(
        std::mem::size_of::<CurrentMount>(),
        2 * std::mem::size_of::<usize>()
    );
}

#[test]
fn context_keys_select_only_their_own_value() {
    let slot = ContextSlot::new(&NUMBER, &17);
    let _mount = MountGuard::install(ptr::null(), Some(&slot));

    assert_eq!(NUMBER.with(|value| *value), Some(17));
    assert!(OTHER_NUMBER.with(|_| ()).is_none());
}

#[test]
fn panicking_fiber_cannot_suspend_before_reaching_its_catch_boundary() {
    struct SuspendOnDrop(Rc<Cell<Option<Result<Resume, SuspendError>>>>);

    impl Drop for SuspendOnDrop {
        fn drop(&mut self) {
            self.0.set(Some(suspend(Suspension::YieldNow)));
        }
    }

    let observed = Rc::new(Cell::new(None));
    let body_observed = Rc::clone(&observed);
    let mut fiber = Fiber::new(MappedStack::new(128 * 1024, 0).unwrap(), move || {
        let _suspend_on_drop = SuspendOnDrop(body_observed);
        panic!("expected fiber panic");
    });

    let first = catch_unwind(AssertUnwindSafe(|| fiber.resume()));
    let suspended = match first {
        Ok(FiberState::Suspended(Suspension::YieldNow)) => {
            assert!(catch_unwind(AssertUnwindSafe(|| fiber.resume())).is_err());
            true
        }
        Ok(state) => panic!("panicking fiber returned unexpected state: {state:?}"),
        Err(_) => false,
    };

    assert!(
        !suspended,
        "panicking fiber transferred control to its carrier"
    );
    assert_eq!(
        observed.take().expect("destructor ran"),
        Err(SuspendError::Panicking)
    );
    assert!(fiber.is_complete());
}
