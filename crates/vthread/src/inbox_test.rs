use crate::{Error, Runtime, control::Shared};
use std::collections::VecDeque;

#[test]
fn bounded_batch_drain_preserves_fifo_and_pending_count() {
    let config = Runtime::builder()
        .carrier_queue_capacity(3)
        .build()
        .expect("config")
        .config();
    let shared = Shared::new(config);
    let scope = shared.begin_scope().expect("scope");
    let inbox = &shared.inboxes[0];
    for name in ["first", "second", "third"] {
        shared.submit(scope, name.into(), || ()).expect("submit");
    }

    let mut drained = VecDeque::new();
    assert_eq!(inbox.drain_into(&mut drained, 2), 2);
    assert_eq!(inbox.pending(), 1);
    assert_eq!(drained.len(), 2);
    assert_eq!(drained.pop_front().unwrap().record.lock().id.get(), 1);
    assert_eq!(drained.pop_front().unwrap().record.lock().id.get(), 2);
    assert_eq!(inbox.pop().unwrap().record.lock().id.get(), 3);
    assert_eq!(inbox.pending(), 0);
}

#[test]
fn queued_starts_coalesce_notifications_until_the_inbox_is_drained() {
    let config = Runtime::builder()
        .carrier_queue_capacity(2)
        .build()
        .expect("config")
        .config();
    let shared = Shared::new(config);
    let scope = shared.begin_scope().expect("scope");
    let inbox = &shared.inboxes[0];
    let empty_epoch = inbox.signal.version();

    shared.submit(scope, "first".into(), || ()).expect("first");
    let queued_epoch = inbox.signal.version();
    assert_ne!(queued_epoch, empty_epoch);
    shared
        .submit(scope, "second".into(), || ())
        .expect("second");
    assert_eq!(inbox.signal.version(), queued_epoch);

    drop(inbox.pop().expect("first packet"));
    drop(inbox.pop().expect("second packet"));
    shared.submit(scope, "third".into(), || ()).expect("third");
    assert_ne!(inbox.signal.version(), queued_epoch);
}

#[test]
fn concurrent_push_and_batch_drain_publish_exact_pending_depth() {
    use std::{sync::Arc, thread, time::Instant};

    const TASKS: usize = 256;
    let config = Runtime::builder()
        .max_vthreads(TASKS)
        .carrier_queue_capacity(TASKS)
        .build()
        .expect("config")
        .config();
    let shared = Arc::new(Shared::new(config));
    let scope = shared.begin_scope().expect("scope");
    let producer = Arc::clone(&shared);
    let submitted = thread::spawn(move || {
        for index in 0..TASKS {
            producer
                .submit(scope, format!("task-{index}"), || ())
                .expect("submit");
        }
    });

    let deadline = Instant::now() + std::time::Duration::from_secs(2);
    let mut drained = VecDeque::new();
    while drained.len() != TASKS {
        shared.inboxes[0].drain_into(&mut drained, 7);
        assert!(Instant::now() < deadline, "timed out draining starts");
        thread::yield_now();
    }
    submitted.join().expect("producer");
    assert_eq!(shared.inboxes[0].pending(), 0);
}

#[test]
fn capacity_probe_does_not_acquire_the_queue_lock() {
    use std::{sync::Arc, sync::mpsc, thread, time::Duration};

    let inbox = Arc::new(crate::inbox::Inbox::new(1, 1, false));
    let probe = Arc::clone(&inbox);
    let state = crate::signal::lock(&inbox.state);
    let (observed, receiver) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || observed.send(probe.can_accept()));
    let result = receiver.recv_timeout(Duration::from_secs(1));
    drop(state);
    worker.join().expect("capacity probe").expect("observer");
    assert!(result.expect("capacity probe acquired the queue lock"));
}

#[test]
fn a_full_or_stopped_ingress_rejects_without_losing_admission() {
    let config = Runtime::builder()
        .carrier_queue_capacity(1)
        .build()
        .expect("config")
        .config();
    let shared = Shared::new(config);
    let scope = shared.begin_scope().expect("scope");
    shared.submit(scope, "first".into(), || ()).expect("first");
    assert!(matches!(
        shared.submit(scope, "full".into(), || ()),
        Err(Error::Capacity {
            resource: crate::error::CapacityResource::CarrierQueue,
            ..
        })
    ));
    assert_eq!(shared.snapshot().active, 1);
    let packet = shared.inboxes[0].pop().expect("packet remains");
    shared.inboxes[0].stop();
    assert!(shared.inboxes[0].push(packet).is_err());
    assert_eq!(shared.inboxes[0].pending(), 0);
}

#[test]
fn distinct_supervisor_aborts_are_not_overwritten_and_finished_scopes_release_slots() {
    use crate::TaskFailure;
    let inbox = crate::inbox::Inbox::new(2, 2, false);
    inbox.abort(1, TaskFailure::SupervisorStopped);
    inbox.abort(2, TaskFailure::ScopeStalled);
    assert_eq!(
        inbox.take_abort(),
        Some((1, TaskFailure::SupervisorStopped))
    );
    assert_eq!(inbox.take_abort(), Some((2, TaskFailure::ScopeStalled)));
    inbox.abort(3, TaskFailure::ScopeStalled);
    inbox.clear_abort(3);
    assert_eq!(inbox.take_abort(), None);
}
