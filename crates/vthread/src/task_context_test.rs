use super::TaskContext;
use crate::{Error, ScopeOptions, options::TaskOptions};

#[test]
fn cleanup_cannot_suspend_even_when_cancellation_is_masked() {
    let context = TaskContext::new(TaskOptions::root(ScopeOptions::default(), 1), 1);
    context.masked.set(1);
    context.closing.set(true);
    assert!(matches!(context.check(), Err(Error::RuntimeStopped)));
}

#[test]
fn task_local_destructors_keep_task_identity_and_finish_before_join() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static DROPS: AtomicUsize = AtomicUsize::new(0);
    struct Value;
    impl Drop for Value {
        fn drop(&mut self) {
            if crate::cancellation_token().is_ok() {
                DROPS.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
    static KEY: crate::TaskLocal<Value> = crate::TaskLocal::new(|| Value);
    let runtime = crate::Runtime::new().unwrap();
    runtime
        .scope(|scope| {
            scope
                .spawn("context owner", || KEY.with(|_| ()))?
                .join()??;
            assert_eq!(DROPS.load(Ordering::SeqCst), 1);
            Ok(())
        })
        .unwrap();
}

#[test]
fn reentrant_initialization_cannot_exceed_task_local_capacity() {
    static INNER: crate::TaskLocal<usize> = crate::TaskLocal::new(|| 1);
    static OUTER: crate::TaskLocal<usize> =
        crate::TaskLocal::new(|| INNER.with(|value| *value).unwrap());
    let runtime = crate::Runtime::builder()
        .task_local_capacity(1)
        .build()
        .unwrap();
    runtime
        .scope(|scope| {
            scope
                .spawn("bounded context", || {
                    assert!(matches!(
                        OUTER.with(|value| *value),
                        Err(Error::TaskLocalCapacity)
                    ));
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn task_local_values_survive_suspension_and_are_not_inherited_by_children() {
    use std::cell::Cell;
    static KEY: crate::TaskLocal<Cell<usize>> = crate::TaskLocal::new(|| Cell::new(0));
    let runtime = crate::Runtime::new().unwrap();
    runtime
        .scope(|scope| {
            scope
                .spawn("parent", || {
                    KEY.with(|parent| {
                        parent.set(42);
                        crate::local_scope(|local| {
                            for index in 1..=2 {
                                local.spawn("isolated", move || {
                                    KEY.with(|value| {
                                        assert_eq!(value.get(), 0);
                                        value.set(index);
                                        for _ in 0..16 {
                                            crate::yield_now().unwrap();
                                            assert_eq!(value.get(), index);
                                        }
                                    })
                                })?;
                            }
                            Ok(())
                        })
                        .unwrap();
                        assert_eq!(parent.get(), 42);
                    })
                    .unwrap();
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn joins_report_panics_from_task_local_destruction() {
    struct BadDrop;
    impl Drop for BadDrop {
        fn drop(&mut self) {
            panic!("task-local destructor");
        }
    }
    static KEY: crate::TaskLocal<BadDrop> = crate::TaskLocal::new(|| BadDrop);
    let runtime = crate::Runtime::new().unwrap();
    let result = runtime.scope(|scope| scope.spawn("late panic", || KEY.with(|_| ()))?.join()?);
    assert!(matches!(result, Err(Error::TaskPanicked { .. })));
    runtime
        .scope(|scope| scope.spawn("survived", || ())?.join())
        .unwrap();
}
