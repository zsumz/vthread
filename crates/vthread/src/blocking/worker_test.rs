use crate::{Error, Runtime, ScopeOptions, blocking, support_test::until};
use std::{
    sync::{atomic::Ordering, mpsc},
    time::{Duration, Instant},
};

#[test]
fn an_unexpected_native_worker_failure_closes_queued_waiters() {
    let runtime = Runtime::builder().blocking_threads(1).build().unwrap();
    let (release, gate) = mpsc::sync_channel(1);
    let result = runtime.run_scope_with(
        ScopeOptions::default().deadline(Instant::now() + Duration::from_millis(500)),
        |scope| {
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
            release.send(()).unwrap();
            let _ = first.join();
            let result = queued.join()?;
            assert!(
                !matches!(result, Err(Error::DeadlineExceeded) | Err(Error::Cancelled)),
                "dead worker stranded its queued waiter"
            );
            assert!(result.is_err());
            Ok(())
        },
    );
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
