use crate::{FiberLease, FiberState, StackPool, fiber_scope};
use std::{
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

#[test]
fn reentrant_resume_is_rejected_without_revoking_the_running_stack() {
    let slot = Rc::new(RefCell::new(None::<FiberLease>));
    let mut pool = StackPool::new(128 * 1024, 0);
    fiber_scope(1, |scope| {
        let current = Rc::clone(&slot);
        let lease = scope
            .spawn(pool.acquire().unwrap(), move || {
                let lease = current.borrow().as_ref().unwrap().clone();
                assert!(catch_unwind(AssertUnwindSafe(|| lease.resume())).is_err());
                assert!(lease.live());
            })
            .unwrap();
        *slot.borrow_mut() = Some(lease.clone());
        assert_eq!(lease.resume(), Some(FiberState::Complete));
        assert!(lease.take_stack().is_some());
    });
}
