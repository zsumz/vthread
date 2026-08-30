use super::fiber_scope;
use crate::{Fiber, FiberState, StackPool, Suspension, suspend};
use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

#[test]
fn escaped_and_forgotten_leases_cannot_outlive_borrowed_values() {
    let mut count = 0;
    let mut pool = StackPool::new(128 * 1024, 0);
    let lease = fiber_scope(1, |scope| {
        let lease = scope
            .spawn(pool.acquire().unwrap(), || {
                count += 1;
                suspend(Suspension::YieldNow).unwrap();
                count += 100;
            })
            .unwrap();
        assert_eq!(
            lease.resume(),
            Some(FiberState::Suspended(Suspension::YieldNow))
        );
        std::mem::forget(lease.clone());
        lease
    });
    assert_eq!(count, 1);
    assert_eq!(lease.resume(), None);
}

#[test]
fn parent_unwind_drops_borrowed_children() {
    struct Guard<'a>(&'a Cell<usize>);
    impl Drop for Guard<'_> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let count = Cell::new(0);
    let mut pool = StackPool::new(128 * 1024, 0);
    let result = catch_unwind(AssertUnwindSafe(|| {
        fiber_scope(2, |scope| {
            for _ in 0..2 {
                let lease = scope
                    .spawn(pool.acquire().unwrap(), || {
                        let _guard = Guard(&count);
                        suspend(Suspension::YieldNow).unwrap();
                    })
                    .unwrap();
                lease.resume();
            }
            panic!("parent");
        })
    }));
    assert!(result.is_err());
    assert_eq!(count.get(), 2);
}

#[test]
fn reclaiming_a_nested_stack_restores_the_parent_yielder() {
    let finished = Rc::new(Cell::new(false));
    let flag = Rc::clone(&finished);
    let mut pool = StackPool::new(128 * 1024, 0);
    let mut parent = Fiber::new(pool.acquire().unwrap(), move || {
        fiber_scope(1, |scope| {
            let child = scope
                .spawn(pool.acquire().unwrap(), || {
                    suspend(Suspension::YieldNow).unwrap();
                })
                .unwrap();
            child.resume();
        });
        suspend(Suspension::YieldNow).unwrap();
        flag.set(true);
    });
    assert_eq!(parent.resume(), FiberState::Suspended(Suspension::YieldNow));
    assert_eq!(parent.resume(), FiberState::Complete);
    assert!(finished.get());
}

#[test]
fn scope_value_remains_live_while_borrowed_destructors_run() {
    struct OnDrop<F: Fn()>(F);
    impl<F: Fn()> Drop for OnDrop<F> {
        fn drop(&mut self) {
            self.0();
        }
    }
    let owners = Cell::new(0);
    let mut pool = StackPool::new(128 * 1024, 0);
    fiber_scope(1, |scope| {
        let lease = scope
            .spawn(pool.acquire().unwrap(), || {
                let _guard = OnDrop(|| owners.set(Rc::strong_count(&scope.registry)));
                suspend(Suspension::YieldNow).unwrap();
            })
            .unwrap();
        lease.resume();
    });
    assert_eq!(
        owners.get(),
        2,
        "both the lexical scope and reclamation guard remain live"
    );
}
