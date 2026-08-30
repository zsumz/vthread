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
    let reactor = Reactor::new(1).unwrap();
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
    let reactor = Reactor::new(1).unwrap();
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
