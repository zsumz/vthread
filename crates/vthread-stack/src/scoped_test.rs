use super::fiber_scope;
use crate::{Fiber, FiberState, StackPool, Suspension, suspend};
use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

#[test]
fn forced_parent_reclamation_preserves_the_backend_unwind_token() {
    let drops = Rc::new(Cell::new(0));
    let child_drops = Rc::clone(&drops);
    struct Count(Rc<Cell<usize>>);
    impl Drop for Count {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let mut pool = StackPool::new(128 * 1024, 0);
    let mut parent = Fiber::new(pool.acquire().unwrap(), move || {
        fiber_scope(1, |scope| {
            let child = scope
                .spawn(pool.acquire().unwrap(), || {
                    let _count = Count(child_drops);
                    suspend(Suspension::YieldNow).unwrap();
                })
                .unwrap();
            child.resume();
            suspend(Suspension::YieldNow).unwrap();
        });
    });
    parent.resume();
    let result = catch_unwind(AssertUnwindSafe(|| drop(parent)));
    assert!(result.is_ok(), "forced unwind became an ordinary panic");
    assert_eq!(drops.get(), 1);
}

#[test]
fn a_running_lease_can_be_inspected_without_borrowing_its_executing_fiber() {
    let slot = Rc::new(RefCell::new(None::<super::FiberLease>));
    let mut pool = StackPool::new(128 * 1024, 0);
    fiber_scope(1, |scope| {
        let current = Rc::clone(&slot);
        let lease = scope
            .spawn(pool.acquire().unwrap(), move || {
                assert!(current.borrow().as_ref().unwrap().live());
            })
            .unwrap();
        *slot.borrow_mut() = Some(lease.clone());
        assert_eq!(lease.resume(), Some(FiberState::Complete));
    });
}

#[test]
fn incomplete_extraction_preserves_the_lease_for_later_completion() {
    let mut pool = StackPool::new(128 * 1024, 0);
    fiber_scope(1, |scope| {
        let lease = scope
            .spawn(pool.acquire().unwrap(), || {
                suspend(Suspension::YieldNow).unwrap();
            })
            .unwrap();
        lease.resume();
        let result = catch_unwind(AssertUnwindSafe(|| lease.take_stack()));
        assert!(
            matches!(result, Ok(None)),
            "incomplete extraction destroyed its fiber"
        );
        assert!(lease.live());
        assert_eq!(lease.resume(), Some(FiberState::Complete));
        assert!(lease.take_stack().is_some());
    });
}

#[test]
fn failed_cleanup_mount_preserves_ownership_for_retry() {
    let mut pool = StackPool::new(128 * 1024, 0);
    fiber_scope(1, |scope| {
        let lease = scope
            .spawn(pool.acquire().unwrap(), || {
                suspend(Suspension::YieldNow).unwrap();
            })
            .unwrap();
        lease.resume();
        let first = Cell::new(true);
        lease.cleanup_context(move || {
            assert!(!first.replace(false), "injected mount failure");
            Box::new(())
        });
        assert!(catch_unwind(AssertUnwindSafe(|| lease.reclaim())).is_err());
        assert!(
            lease.live(),
            "mount failure lost ownership of the suspended stack"
        );
        lease.reclaim();
        assert!(!lease.live());
    });
}

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
fn reclaiming_a_nested_stack_restores_the_parent_mount() {
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
