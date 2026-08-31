use crate::{Runtime, blocking, support_test::until};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

#[test]
fn services_are_bounded_and_shutdown_joins_every_worker() {
    let runtime = Runtime::builder()
        .io_capacity(3)
        .blocking_threads(1)
        .blocking_capacity(2)
        .build()
        .unwrap();
    let snapshot = runtime.snapshot().services;
    assert_eq!(snapshot.readiness_capacity, 3);
    assert_eq!(snapshot.blocking_capacity, 2);
    runtime.shutdown().unwrap();
    let snapshot = runtime.snapshot().services;
    assert_eq!(snapshot.readiness_registered, 0);
    assert_eq!(snapshot.blocking_running, 0);
    assert!(!snapshot.readiness_failed);
    assert!(Runtime::builder().io_capacity(0).build().is_err());
    assert!(Runtime::builder().blocking_threads(0).build().is_err());
    assert!(Runtime::builder().blocking_capacity(0).build().is_err());
}

#[test]
fn owned_native_progress_is_not_misclassified_as_a_stalled_scope() {
    let runtime = Runtime::builder()
        .stall_policy(crate::StallPolicy::AbortAfter(Duration::from_millis(10)))
        .build()
        .unwrap();
    let started = Arc::new(AtomicBool::new(false));
    let worker_started = Arc::clone(&started);
    let controller = thread::spawn(move || {
        until(|| started.load(Ordering::Acquire));
        thread::park_timeout(Duration::from_millis(50));
    });
    runtime
        .scope(|scope| {
            scope
                .spawn("native", move || {
                    blocking::run(move || {
                        worker_started.store(true, Ordering::Release);
                        controller.join().unwrap();
                        42
                    })
                })?
                .join()??;
            Ok(())
        })
        .unwrap();
}

#[test]
fn a_native_worker_cannot_shut_down_the_runtime_that_owns_it() {
    let runtime = Arc::new(Runtime::builder().blocking_threads(1).build().unwrap());
    runtime
        .scope(|scope| {
            let shared = Arc::clone(&runtime);
            let task = scope.spawn("worker-shutdown", move || {
                blocking::run(move || shared.shutdown())
            })?;
            assert!(matches!(
                task.join()??,
                Err(crate::Error::InsideBlockingWorker)
            ));
            Ok(())
        })
        .unwrap();
    assert_eq!(runtime.snapshot().active, 0);
    runtime
        .scope(|scope| {
            assert_eq!(scope.spawn("reuse", || blocking::run(|| 42))?.join()??, 42);
            Ok(())
        })
        .unwrap();
}
