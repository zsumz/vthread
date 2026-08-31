use crate::{Error, Runtime, net::unix::UnixStream, support_test::until};
use std::sync::Arc;

#[test]
fn cancelled_readiness_releases_descriptor_registrations_before_reuse() {
    let runtime = Runtime::builder().io_capacity(1).build().unwrap();
    let (reader, _writer) = UnixStream::pair().unwrap();
    let reader = Arc::new(reader);
    for _ in 0..32 {
        runtime
            .run_scope(|scope| {
                let socket = Arc::clone(&reader);
                let mut child = scope.spawn("read", move || socket.read(&mut [0; 1]))?;
                until(|| runtime.snapshot().services.readiness_waits == 1);
                scope.cancel();
                assert!(matches!(child.join()?, Err(Error::Cancelled)));
                Ok(())
            })
            .unwrap();
        until(|| runtime.snapshot().services.readiness_registered == 0);
    }
    runtime
        .run_scope(|scope| {
            let socket = Arc::clone(&reader);
            let mut child = scope.spawn("shutdown", move || socket.read(&mut [0; 1]))?;
            until(|| runtime.snapshot().services.readiness_waits == 1);
            runtime.shutdown()?;
            assert!(matches!(child.join(), Err(Error::TaskAborted { .. })));
            Ok(())
        })
        .unwrap();
    assert_eq!(runtime.snapshot().services.readiness_registered, 0);
    assert!(!runtime.snapshot().services.readiness_failed);
}

#[test]
fn driver_fault_unblocks_tasks_releases_descriptors_and_reports_the_cause() {
    let runtime = Runtime::new().unwrap();
    let (reader, _writer) = UnixStream::pair().unwrap();
    runtime
        .run_scope(|scope| {
            let mut child = scope.spawn("driver-fault", move || reader.read(&mut [0; 1]))?;
            until(|| runtime.snapshot().services.readiness_waits == 1);
            let inner = &runtime.shared.services.get().unwrap().reactor.inner;
            inner
                .fail_wait
                .store(true, std::sync::atomic::Ordering::Release);
            inner.waker.wake().unwrap();
            assert!(matches!(child.join()?, Err(Error::ReadinessFailed)));
            Ok(())
        })
        .unwrap();
    until(|| runtime.snapshot().services.readiness_registered == 0);
    assert_eq!(
        runtime.snapshot().services.readiness_error.as_deref(),
        Some("injected readiness wait failure")
    );
}
