use crate::{Error, Runtime, blocking, support_test::until};
use std::{sync::mpsc, thread, time::Duration};

#[test]
fn native_work_releases_the_single_carrier_and_isolates_panics() {
    let runtime = Runtime::new().unwrap();
    runtime
        .run_scope(|scope| {
            let (send, receive) = mpsc::sync_channel(1);
            let mut waiter = scope.spawn("native", move || {
                let carrier = thread::current().id();
                let worker = blocking::run(move || {
                    receive.recv_timeout(Duration::from_secs(5)).unwrap();
                    thread::current().id()
                })?;
                assert_ne!(carrier, worker);
                Ok::<_, Error>(())
            })?;
            until(|| runtime.snapshot().services.blocking_running == 1);
            scope
                .spawn("progress", move || send.send(()).unwrap())?
                .join()?;
            waiter.join()??;
            let mut panic = scope.spawn("panic", || blocking::run(|| panic!("native panic")))?;
            assert!(matches!(panic.join()?, Err(Error::BlockingPanicked(_))));
            assert_eq!(scope.spawn("reuse", || blocking::run(|| 42))?.join()??, 42);
            Ok(())
        })
        .unwrap();
    assert!(matches!(blocking::run(|| 42), Err(Error::OutsideVThread)));
}

#[test]
fn cancelling_a_running_call_returns_before_its_late_result_is_destroyed() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct Value(Arc<AtomicUsize>);
    impl Drop for Value {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let runtime = Runtime::new().unwrap();
    let drops = Arc::new(AtomicUsize::new(0));
    let (send, receive) = mpsc::sync_channel(1);
    runtime
        .run_scope(|scope| {
            let tracked = Arc::clone(&drops);
            let mut caller = scope.spawn("cancel", move || {
                blocking::run(move || {
                    receive.recv_timeout(Duration::from_secs(5)).unwrap();
                    Value(tracked)
                })
            })?;
            until(|| runtime.snapshot().services.blocking_running == 1);
            scope.cancel();
            assert!(matches!(caller.join()?, Err(Error::Cancelled)));
            assert_eq!(drops.load(Ordering::SeqCst), 0);
            send.send(()).unwrap();
            Ok(())
        })
        .unwrap();
    runtime.shutdown().unwrap();
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}
#[test]
fn rejected_native_submission_destroys_consumed_captures_on_the_caller() {
    use std::{sync::mpsc, thread, time::Duration};
    struct Capture(mpsc::SyncSender<thread::ThreadId>);
    impl Drop for Capture {
        fn drop(&mut self) {
            self.0.send(thread::current().id()).unwrap();
        }
    }
    let runtime = crate::Runtime::builder()
        .blocking_threads(1)
        .blocking_capacity(1)
        .build()
        .unwrap();
    let (release, gate) = mpsc::sync_channel(1);
    let (dropped, observed) = mpsc::sync_channel(1);
    runtime
        .run_scope(|scope| {
            let mut running = scope.spawn("holds capacity", move || {
                super::run(move || gate.recv_timeout(Duration::from_secs(5)).unwrap())
            })?;
            crate::support_test::until(|| runtime.snapshot().services().blocking_running() == 1);
            let mut rejected = scope.spawn("rejected", move || {
                let caller = thread::current().id();
                let capture = Capture(dropped);
                let result = super::run(move || drop(capture));
                assert!(matches!(
                    result,
                    Err(crate::Error::Capacity {
                        resource: crate::error::CapacityResource::NativeJobs,
                        limit: 1
                    })
                ));
                caller
            })?;
            let caller = rejected.join()?;
            let disposer = observed.recv_timeout(Duration::from_secs(5)).unwrap();
            release.send(()).unwrap();
            running.join()??;
            assert_eq!(disposer, caller);
            Ok(())
        })
        .unwrap();
}
