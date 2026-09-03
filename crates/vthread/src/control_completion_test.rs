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
fn completion_batch_retires_scope_and_carrier_load_together() {
    let config = Runtime::builder()
        .carriers(1)
        .build()
        .expect("config")
        .config();
    let shared = Shared::new(config);
    let scope = shared.begin_scope().unwrap();
    let records = (0..3)
        .map(|index| {
            shared
                .reserve(scope, format!("task {index}"), None)
                .unwrap()
        })
        .collect::<Vec<_>>();
    let mut completions = super::CompletionBatch::new();
    for record in &records {
        completions.push(shared.prepare_completion(record, None).unwrap());
    }

    let progress = shared.scope_progress(scope);
    shared.publish_completions(&completions, &progress);

    let snapshot = shared.snapshot();
    assert_eq!(snapshot.active, 0);
    assert_eq!(snapshot.carriers[0].active, 0);
    let report = shared.scope_report(scope);
    assert_eq!(report.completed, 3);
    assert_eq!(report.panicked, 0);
    assert_eq!(report.aborted, 0);
    shared.finish_scope(scope);
}

#[test]
fn target_waiter_is_notified_before_its_scope_drains() {
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().unwrap();
    let target = shared.reserve(scope, "target".into(), None).unwrap();
    let sibling = shared.reserve(scope, "sibling".into(), None).unwrap();
    let waiting_for = Arc::clone(&target);
    let observer = Arc::clone(&shared);
    let (sent, received) = mpsc::sync_channel(1);
    let waiter = std::thread::spawn(move || sent.send(observer.wait(scope, Some(&waiting_for))));

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
