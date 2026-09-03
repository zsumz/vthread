use crate::{Runtime, RuntimeConfig, StallPolicy, TaskStatus, control::Shared};
use std::{
    sync::{Arc, atomic::Ordering, mpsc},
    time::{Duration, Instant},
};

#[test]
fn completion_is_committed_once() {
    let shared = Shared::new(RuntimeConfig::default());
    let scope = shared.begin_scope().unwrap();
    let record = shared.reserve(scope, "done".into(), None).unwrap();
    shared.complete(&record, None);
    shared.complete(&record, None);
    assert_eq!(record.lock().status, TaskStatus::Completed);
}

#[test]
fn completion_notifications_are_coalesced_until_scope_drains() {
    let shared = Shared::new(RuntimeConfig::default());
    let scope = shared.begin_scope().unwrap();
    let first = shared.reserve(scope, "first".into(), None).unwrap();
    let second = shared.reserve(scope, "second".into(), None).unwrap();
    let last = shared.reserve(scope, "last".into(), None).unwrap();
    let observed = shared.changed.version();

    shared.complete(&first, None);
    shared.complete(&second, None);
    assert_eq!(shared.changed.version(), observed);

    shared.complete(&last, None);
    assert!(shared.changed.version() > observed);
}

#[test]
fn target_waiter_is_notified_before_its_scope_drains() {
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().unwrap();
    let target = shared.reserve(scope, "target".into(), None).unwrap();
    let sibling = shared.reserve(scope, "sibling".into(), None).unwrap();
    let target_id = target.lock().id;
    let observer = Arc::clone(&shared);
    let (sent, received) = mpsc::sync_channel(1);
    let waiter = std::thread::spawn(move || sent.send(observer.wait(scope, Some(target_id))));

    let deadline = Instant::now() + Duration::from_secs(1);
    while shared.target_waiters.load(Ordering::SeqCst) == 0 {
        assert!(Instant::now() < deadline, "target waiter did not register");
        std::thread::yield_now();
    }
    shared.complete(&target, None);
    let result = received
        .recv_timeout(Duration::from_secs(1))
        .expect("target completion did not wake its waiter");
    result.unwrap();

    shared.complete(&sibling, None);
    waiter.join().unwrap().unwrap();
    shared.finish_scope(scope);
}

#[test]
fn stall_detection_observes_each_completion() {
    let config = Runtime::builder()
        .stall_policy(StallPolicy::ReportAfter(Duration::from_secs(1)))
        .build()
        .unwrap()
        .config();
    let shared = Shared::new(config);
    let scope = shared.begin_scope().unwrap();
    let first = shared.reserve(scope, "first".into(), None).unwrap();
    let last = shared.reserve(scope, "last".into(), None).unwrap();
    let observed = shared.changed.version();

    shared.complete(&first, None);
    assert!(shared.changed.version() > observed);
    shared.complete(&last, None);
}
