use std::sync::Arc;

use crate::{
    TaskId,
    task_slab::TaskKey,
    wait::{NotifyResult, ResourceSelection, WaitBegin, WaitCell, WaitHub, WakeCause},
};

fn parked(cell: &WaitCell, hub: &Arc<WaitHub>) -> vthread_stack::ParkToken {
    match cell
        .begin(TaskId::new(1), TaskKey::owned(0), hub, None)
        .expect("begin wait")
    {
        WaitBegin::Park { request, .. } => request.token(),
        WaitBegin::Immediate(cause) => panic!("unexpected immediate wake: {cause:?}"),
    }
}

#[test]
fn notification_fast_path_preserves_non_ready_winners() {
    let cell = WaitCell::new();
    let hub = Arc::new(WaitHub::new(1, Arc::default()));
    let ready = parked(&cell, &hub);
    assert_eq!(cell.notify(), NotifyResult::Woke);
    assert!(hub.pop_wake().is_some());
    assert_eq!(cell.finish_plain_ready(ready).unwrap(), WakeCause::Ready);

    let cancelled = parked(&cell, &hub);
    assert!(cell.cancel());
    assert!(hub.pop_wake().is_some());
    assert_eq!(
        cell.finish_plain_ready(cancelled).unwrap(),
        WakeCause::Cancelled
    );
}

#[test]
fn permit_fast_path_preserves_other_winners() {
    let cell = WaitCell::new();
    let hub = Arc::new(WaitHub::new(1, Arc::default()));
    let ready = parked(&cell, &hub);
    assert!(cell.offer_resource(ResourceSelection::Permit));
    assert!(hub.pop_wake().is_some());
    assert_eq!(cell.finish_permit_ready(ready).unwrap(), WakeCause::Ready);

    let cancelled = parked(&cell, &hub);
    assert!(cell.cancel());
    assert!(hub.pop_wake().is_some());
    assert_eq!(
        cell.finish_permit_ready(cancelled).unwrap(),
        WakeCause::Cancelled
    );
}
