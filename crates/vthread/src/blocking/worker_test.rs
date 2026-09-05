use crate::{Error, Runtime, blocking, support_test::until};
use std::{
    sync::{atomic::Ordering, mpsc},
    time::Duration,
};

#[test]
fn an_unexpected_native_worker_failure_closes_queued_waiters() {
    let runtime = Runtime::builder().blocking_threads(1).build().unwrap();
    let (release, gate) = mpsc::sync_channel(1);
    let result = runtime.run_scope(|scope| {
        let mut first = scope.spawn("running", move || {
            blocking::run(move || gate.recv_timeout(Duration::from_secs(5)).unwrap())
        })?;
        until(|| runtime.snapshot().services.blocking_running == 1);
        let mut queued = scope.spawn("queued", || {
            blocking::run(|| panic!("failed pool ran a queued job"))
        })?;
        until(|| runtime.snapshot().services.blocking_queued == 1);
        runtime
            .shared
            .services
            .get()
            .unwrap()
            .blocking
            .inner
            .fail_worker
            .store(true, Ordering::Release);
        // Do not race the expected Closed winner against a scope deadline.
        // Arm a separate rescue only after the running/queued states exist.
        let cancellation = scope.cancellation_token();
        let (finished, observed_finish) = mpsc::sync_channel(1);
        let watchdog = std::thread::spawn(move || {
            if observed_finish.recv_timeout(Duration::from_secs(5)).is_ok() {
                false
            } else {
                cancellation.cancel();
                true
            }
        });
        release.send(()).unwrap();
        let _ = first.join();
        let result = queued.join();
        let _ = finished.send(());
        assert!(
            !watchdog.join().unwrap(),
            "dead worker stranded its queued waiter"
        );
        assert!(matches!(result?, Err(Error::BlockingFailed)));
        Ok(())
    });
    assert!(
        result.is_ok(),
        "worker failure escaped queue closure: {result:?}"
    );
    until(|| runtime.snapshot().services.blocking_queued == 0);
    assert!(runtime.snapshot().services.blocking_failed);
    runtime
        .run_scope(|scope| {
            assert!(matches!(
                scope.spawn("rejected", || blocking::run(|| ()))?.join()?,
                Err(Error::BlockingFailed)
            ));
            Ok(())
        })
        .unwrap();
    let Err(Error::ShutdownFailed(report)) = runtime.shutdown() else {
        panic!("native worker failure must survive shutdown");
    };
    assert!(
        report
            .failures
            .entries()
            .iter()
            .any(|failure| failure.component() == crate::ThreadComponent::NativeWorker)
    );
}
