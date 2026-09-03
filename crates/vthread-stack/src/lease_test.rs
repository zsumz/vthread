use crate::{ContextKey, FiberLease, FiberState, Resume, StackPool, fiber_scope};
use std::{
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

static LEASE_CONTEXT: ContextKey<u64> = ContextKey::new();

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

#[test]
fn lease_forwards_typed_context_without_extending_its_mount() {
    let mut pool = StackPool::new(128 * 1024, 0);
    fiber_scope(1, |scope| {
        let lease = scope
            .spawn(pool.acquire().unwrap(), || {
                assert_eq!(LEASE_CONTEXT.with(|value| *value), Some(23));
            })
            .unwrap();
        assert_eq!(
            lease.resume_with_context(Resume::Continue, &LEASE_CONTEXT, &23),
            Some(FiberState::Complete)
        );
        assert!(LEASE_CONTEXT.with(|_| ()).is_none());
        assert!(lease.take_stack().is_some());
    });
}
