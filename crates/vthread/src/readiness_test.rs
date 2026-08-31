use super::Reactor;
use crate::{
    Error, TaskId,
    support_test::until,
    wait::{WaitBegin, WaitCell, WaitHub, WakeCause},
};
use std::{
    io::Write,
    os::{fd::AsFd, unix::net::UnixStream},
    sync::Arc,
};

#[test]
fn registration_is_bounded_and_remote_readiness_selects_the_exact_generation() {
    let reactor = Reactor::new(1, std::sync::Weak::new()).unwrap();
    let (reader, mut writer) = UnixStream::pair().unwrap();
    reader.set_nonblocking(true).unwrap();
    let cell = WaitCell::new();
    let hub = Arc::new(WaitHub::new(2, Arc::default()));
    let WaitBegin::Park(request) = cell.begin(TaskId::new(1), &hub, None).unwrap() else {
        panic!("park");
    };
    let token = request.token();
    let lease = reactor
        .register(
            reader.as_fd(),
            zio::Interest::READABLE,
            token,
            cell.registration(),
        )
        .unwrap();
    assert!(matches!(
        reactor.register(
            reader.as_fd(),
            zio::Interest::READABLE,
            token,
            cell.registration()
        ),
        Err(Error::WaitQueueFull { limit: 1 })
    ));
    writer.write_all(b"ready").unwrap();
    until(|| hub.pending() == 1);
    let wake = hub.pop_wake().unwrap();
    assert_eq!(wake.token, token);
    assert_eq!(wake.cause, WakeCause::Ready);
    assert_eq!(cell.finish(token).unwrap(), WakeCause::Ready);
    drop(lease);
    reactor.stop();
    reactor.join();
    assert_eq!(
        reactor
            .inner
            .registered
            .load(std::sync::atomic::Ordering::Acquire),
        0
    );
}

#[test]
fn driver_failure_closes_waits_and_rejects_later_registration() {
    let reactor = Reactor::new(1, std::sync::Weak::new()).unwrap();
    let (reader, _writer) = UnixStream::pair().unwrap();
    let cell = WaitCell::new();
    let hub = Arc::new(WaitHub::new(1, Arc::default()));
    let WaitBegin::Park(request) = cell.begin(TaskId::new(1), &hub, None).unwrap() else {
        panic!("park");
    };
    let _lease = reactor
        .register(
            reader.as_fd(),
            zio::Interest::READABLE,
            request.token(),
            cell.registration(),
        )
        .unwrap();
    reactor.inner.close(Some("injected failure".to_owned()));
    until(|| hub.pending() == 1);
    assert_eq!(hub.pop_wake().unwrap().cause, WakeCause::Closed);
    assert!(matches!(reactor.check(), Err(Error::ReadinessFailed)));
}

#[test]
fn shutdown_closes_native_cleanup_ownership_before_waking_carriers() {
    use std::{sync::mpsc, thread, time::Duration};
    struct Capture(mpsc::SyncSender<String>);
    impl Drop for Capture {
        fn drop(&mut self) {
            self.0
                .send(thread::current().name().unwrap_or("unnamed").to_owned())
                .unwrap();
        }
    }
    let runtime = Arc::new(
        crate::Runtime::builder()
            .blocking_threads(1)
            .build()
            .unwrap(),
    );
    let (release, gate) = mpsc::sync_channel(1);
    let (dropped, owner) = mpsc::sync_channel(1);
    runtime
        .scope(|scope| {
            let running = scope.spawn("running", move || {
                crate::blocking::run(move || {
                    gate.recv_timeout(Duration::from_secs(5)).unwrap();
                })
            })?;
            until(|| runtime.snapshot().services.blocking_running == 1);
            let capture = Capture(dropped);
            let queued = scope.spawn("queued", move || {
                crate::blocking::run(move || {
                    let _capture = capture;
                    panic!("a stopped queued body must not execute");
                })
            })?;
            until(|| runtime.snapshot().services.blocking_queued == 1);
            let services = runtime.shared.services.get().unwrap();
            // Hold an unrelated service lock to expose the stop-order window deterministically.
            let readiness = crate::signal::lock(&services.reactor.inner.state);
            let remote = Arc::clone(&runtime);
            let stopper = thread::spawn(move || remote.request_shutdown());
            until(|| runtime.shared.inboxes[0].stopped() || services.blocking.is_stopped());
            let premature = if !services.blocking.is_stopped() {
                owner.recv_timeout(Duration::from_secs(1)).ok()
            } else {
                None
            };
            drop(readiness);
            stopper.join().unwrap();
            release.send(()).unwrap();
            runtime.shutdown()?;
            let _ = running.join();
            let _ = queued.join();
            let owner =
                premature.unwrap_or_else(|| owner.recv_timeout(Duration::from_secs(1)).unwrap());
            assert!(
                owner.starts_with("vthread-blocking-"),
                "queued cleanup escaped onto {owner}"
            );
            Ok(())
        })
        .unwrap();
}
