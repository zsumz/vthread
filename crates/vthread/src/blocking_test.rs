use crate::{Error, Runtime, blocking, support_test::until};
use std::{sync::mpsc, thread, time::Duration};

#[test]
fn native_work_releases_the_single_carrier_and_isolates_panics() {
    let runtime = Runtime::new().unwrap();
    runtime
        .scope(|scope| {
            let (send, receive) = mpsc::sync_channel(1);
            let waiter = scope.spawn("native", move || {
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
            let panic = scope.spawn("panic", || blocking::run(|| panic!("native panic")))?;
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
        .scope(|scope| {
            let tracked = Arc::clone(&drops);
            let caller = scope.spawn("cancel", move || {
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
