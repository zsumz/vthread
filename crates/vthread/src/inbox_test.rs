use crate::{Error, Runtime, control::Shared};

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
fn capacity_probe_does_not_acquire_the_queue_lock() {
    use std::{sync::Arc, sync::mpsc, thread, time::Duration};

    let inbox = Arc::new(crate::inbox::Inbox::new(1, 1));
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
    let inbox = crate::inbox::Inbox::new(2, 2);
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
