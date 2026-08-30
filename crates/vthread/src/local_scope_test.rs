use crate::{Runtime, local_scope, yield_now};
use std::{cell::Cell, rc::Rc, thread};

#[test]
fn borrowed_non_send_children_and_results_stay_on_the_parent_carrier() {
    let runtime = Runtime::builder().carriers(2).build().unwrap();
    runtime
        .scope(|scope| {
            scope
                .spawn("parent", || {
                    let owner = thread::current().id();
                    let value = Rc::new(Cell::new(0));
                    let mut output = String::new();
                    local_scope(|local| {
                        let left = local.spawn("borrowed", || {
                            yield_now().unwrap();
                            assert_eq!(thread::current().id(), owner);
                            value.set(42);
                            output.push_str("done");
                            Rc::clone(&value)
                        })?;
                        let right = local.spawn("sibling", || {
                            yield_now().unwrap();
                            thread::current().id()
                        })?;
                        assert_eq!(right.join()?, owner);
                        assert_eq!(left.join()?.get(), 42);
                        Ok(())
                    })
                    .unwrap();
                    assert_eq!(output, "done");
                    assert_eq!(value.get(), 42);
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn nested_scopes_inherit_the_earliest_deadline_and_isolate_child_cancellation() {
    use crate::{Error, local_scope_with_deadline};
    use std::time::{Duration, Instant};
    let runtime = Runtime::new().unwrap();
    runtime
        .scope(|scope| {
            scope
                .spawn("parent", || {
                    let early = Instant::now() + Duration::from_secs(5);
                    local_scope_with_deadline(early, |local| {
                        let child = local.spawn("nested", || {
                            local_scope_with_deadline(early + Duration::from_secs(10), |nested| {
                                assert_eq!(nested.deadline(), Some(early));
                                nested.cancel();
                                Ok(())
                            })
                        })?;
                        assert!(matches!(child.join()?, Err(Error::Cancelled)));
                        assert!(!local.cancellation_token().is_cancelled());
                        Ok(())
                    })
                    .unwrap();
                    assert!(!crate::cancellation_token().unwrap().is_cancelled());
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn shutdown_reclaims_nested_borrowed_children_before_the_parent_environment() {
    use crate::{Error, park_pair, support_test::until};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct Guard<'a>(&'a AtomicUsize);
    impl Drop for Guard<'_> {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let runtime = Runtime::new().unwrap();
    let drops = Arc::new(AtomicUsize::new(0));
    runtime
        .scope(|scope| {
            let tracked = Arc::clone(&drops);
            let parent = scope.spawn("parent", move || {
                local_scope(|local| {
                    local.spawn("borrowed", || {
                        let _guard = Guard(&tracked);
                        local_scope(|nested| {
                            nested.spawn("grandchild", || {
                                let _guard = Guard(&tracked);
                                let (parker, _waker) = park_pair();
                                parker.park()
                            })?;
                            Ok(())
                        })
                    })?;
                    Ok(())
                })
            })?;
            until(|| scope.snapshot().parked == 3);
            let report = runtime.shutdown()?;
            assert_eq!(report.aborted, 3);
            assert!(matches!(parent.join(), Err(Error::TaskAborted { .. })));
            assert_eq!(drops.load(Ordering::SeqCst), 2);
            assert_eq!(scope.snapshot().active, 0);
            Ok(())
        })
        .unwrap();
}

#[test]
fn parent_panic_drains_local_children_before_borrowed_data_can_be_reused() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let runtime = Runtime::new().unwrap();
    runtime
        .scope(|scope| {
            scope
                .spawn("parent", || {
                    let value = Cell::new(0);
                    let outcome = catch_unwind(AssertUnwindSafe(|| {
                        local_scope::<()>(|local| {
                            local.spawn("borrowed", || {
                                value.set(42);
                            })?;
                            panic!("parent body failed");
                        })
                    }));
                    assert!(outcome.is_err());
                    assert_eq!(value.get(), 42);
                    yield_now().unwrap();
                })?
                .join()
        })
        .unwrap();
}
