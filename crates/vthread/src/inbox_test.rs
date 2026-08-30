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
