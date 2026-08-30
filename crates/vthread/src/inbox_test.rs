use crate::{Error, Runtime, control::Shared};

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
        Err(Error::CarrierQueueFull)
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
