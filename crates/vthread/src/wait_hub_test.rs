use std::sync::{Arc, Weak};

use vthread_stack::ParkToken;

use super::WaitHub;
use crate::{
    Error, TaskId,
    wait::{WaitBegin, WaitCell, WakeCause, WakeNotice},
};

#[test]
fn capacity_is_reserved_before_parking_and_released_on_rollback() {
    let hub = Arc::new(WaitHub::new(1, Arc::default()));
    let first = WaitCell::new();
    let second = WaitCell::new();
    let WaitBegin::Park(request) = first.begin(TaskId::new(1), &hub, None).expect("first") else {
        panic!("expected a park");
    };
    assert!(matches!(
        second.begin(TaskId::new(2), &hub, None),
        Err(Error::Capacity {
            resource: crate::error::CapacityResource::Waiters,
            limit: 1
        })
    ));
    first.rollback(request.token());
    assert!(second.begin(TaskId::new(2), &hub, None).is_ok());
}

#[test]
fn duplicates_and_stale_generations_cannot_fill_the_inbox() {
    let hub = WaitHub::new(1, Arc::default());
    let token = ParkToken::new(1, 2);
    hub.register(token, Weak::new()).expect("reserve");
    let notice = WakeNotice {
        token,
        task: TaskId::new(1),
        cause: WakeCause::Ready,
    };
    hub.enqueue(notice);
    for _ in 0..100 {
        hub.enqueue(notice);
        hub.enqueue(WakeNotice {
            token: ParkToken::new(1, 1),
            ..notice
        });
    }
    assert_eq!(hub.pending(), 1);
    assert_eq!(hub.pending_tasks(), vec![TaskId::new(1)]);
    assert_eq!(hub.stale(), 200);
    assert_eq!(hub.pop_wake(), Some(notice));
    assert!(hub.pop_wake().is_none());
}
