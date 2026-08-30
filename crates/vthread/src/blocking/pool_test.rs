use crate::{Error, Runtime, blocking, support_test::until};
use std::{sync::mpsc, time::Duration};

#[test]
fn saturation_rejects_and_cancellation_removes_queued_work() {
    let runtime = Runtime::builder()
        .blocking_threads(1)
        .blocking_capacity(2)
        .build()
        .unwrap();
    let (release, receive) = mpsc::sync_channel(1);
    runtime
        .scope(|scope| {
            let first = scope.spawn("running", move || {
                blocking::run(move || receive.recv_timeout(Duration::from_secs(5)).unwrap())
            })?;
            until(|| runtime.snapshot().services.blocking_running == 1);
            let queued = scope.spawn("queued", || {
                blocking::run(|| panic!("cancelled queued job ran"))
            })?;
            until(|| runtime.snapshot().services.blocking_queued == 1);
            let rejected = scope.spawn("rejected", || blocking::run(|| 3))?;
            assert!(matches!(rejected.join()?, Err(Error::BlockingCapacity)));
            scope.cancel();
            assert!(matches!(queued.join()?, Err(Error::Cancelled)));
            assert!(matches!(first.join()?, Err(Error::Cancelled)));
            assert_eq!(runtime.snapshot().services.blocking_queued, 0);
            release.send(()).unwrap();
            Ok(())
        })
        .unwrap();
    runtime.shutdown().unwrap();
    assert_eq!(runtime.snapshot().services.blocking_running, 0);
}

#[test]
fn late_result_destructor_panic_does_not_kill_the_worker() {
    struct PanicDrop;
    impl Drop for PanicDrop {
        fn drop(&mut self) {
            panic!("late result drop");
        }
    }
    let runtime = Runtime::builder().blocking_threads(1).build().unwrap();
    let (release, receive) = mpsc::sync_channel(1);
    runtime
        .scope(|scope| {
            let job = scope.spawn("abandon", move || {
                blocking::run(move || {
                    receive.recv_timeout(Duration::from_secs(5)).unwrap();
                    PanicDrop
                })
            })?;
            until(|| runtime.snapshot().services.blocking_running == 1);
            scope.cancel();
            assert!(matches!(job.join()?, Err(Error::Cancelled)));
            release.send(()).unwrap();
            Ok(())
        })
        .unwrap();
    until(|| runtime.snapshot().services.blocking_running == 0);
    assert_eq!(runtime.snapshot().services.blocking_panics, 1);
    runtime
        .scope(|scope| {
            assert_eq!(scope.spawn("reuse", || blocking::run(|| 7))?.join()??, 7);
            Ok(())
        })
        .unwrap();
}

#[test]
fn a_panicking_panic_payload_does_not_leave_its_caller_parked() {
    use crate::ScopeOptions;
    use std::time::Instant;
    struct Payload;
    impl Drop for Payload {
        fn drop(&mut self) {
            panic!("payload destructor");
        }
    }
    let runtime = Runtime::builder().blocking_threads(1).build().unwrap();
    runtime
        .scope_with(
            ScopeOptions::default().deadline(Instant::now() + Duration::from_millis(100)),
            |scope| {
                let job = scope.spawn("panic-payload", || {
                    blocking::run(|| std::panic::panic_any(Payload))
                })?;
                assert!(matches!(job.join()?, Err(Error::BlockingPanicked(_))));
                Ok(())
            },
        )
        .unwrap();
}

#[test]
fn shutdown_discards_queued_captures_outside_runtime_metadata_locks() {
    use std::sync::{
        Arc, Weak,
        atomic::{AtomicUsize, Ordering},
    };
    struct Capture(Weak<Runtime>, Arc<AtomicUsize>);
    impl Drop for Capture {
        fn drop(&mut self) {
            self.0.upgrade().unwrap().snapshot();
            self.1.fetch_add(1, Ordering::SeqCst);
        }
    }
    let runtime = Arc::new(
        Runtime::builder()
            .blocking_threads(1)
            .blocking_capacity(2)
            .build()
            .unwrap(),
    );
    let drops = Arc::new(AtomicUsize::new(0));
    let (release, receive) = mpsc::sync_channel(1);
    runtime
        .scope(|scope| {
            let first = scope.spawn("running", move || {
                blocking::run(move || receive.recv_timeout(Duration::from_secs(5)).unwrap())
            })?;
            until(|| runtime.snapshot().services.blocking_running == 1);
            let captured = Capture(Arc::downgrade(&runtime), Arc::clone(&drops));
            let queued = scope.spawn("queued", move || {
                blocking::run(move || {
                    let _captured = captured;
                    panic!("shutdown queued work must not execute");
                })
            })?;
            until(|| runtime.snapshot().services.blocking_queued == 1);
            runtime.shared.request_stop();
            release.send(()).unwrap();
            runtime.shutdown()?;
            assert_eq!(drops.load(Ordering::SeqCst), 1);
            assert!(matches!(first.join(), Err(Error::TaskAborted { .. })));
            assert!(matches!(queued.join(), Err(Error::TaskAborted { .. })));
            Ok(())
        })
        .unwrap();
}
