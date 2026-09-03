use std::sync::Arc;

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
    assert_eq!(hub.reserved(), 1);
    assert!(matches!(
        second.begin(TaskId::new(2), &hub, None,),
        Err(Error::Capacity {
            resource: crate::error::CapacityResource::Waiters,
            limit: 1
        })
    ));
    first.rollback(request.token());
    assert_eq!(hub.reserved(), 0);
    let WaitBegin::Park(second_request) = second.begin(TaskId::new(2), &hub, None).unwrap() else {
        panic!("expected a second park");
    };
    second.rollback(second_request.token());
    assert_eq!(hub.reserved(), 0);
}

#[test]
fn duplicates_and_stale_generations_cannot_fill_the_inbox() {
    let hub = WaitHub::new(1, Arc::default());
    let token = ParkToken::new(1, 2);
    hub.reserve().expect("reserve");
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
    hub.release();
}

#[test]
fn queued_wakes_coalesce_notifications_until_drained() {
    let signal = Arc::default();
    let hub = WaitHub::new(2, Arc::clone(&signal));
    let first = ParkToken::new(1, 1);
    let second = ParkToken::new(2, 1);
    hub.reserve().expect("first reservation");
    hub.reserve().expect("second reservation");
    let empty = signal.version();
    std::thread::scope(|threads| {
        threads.spawn(|| signal.wait(empty, None));
        while signal.waiting() == 0 {
            std::thread::yield_now();
        }
        hub.enqueue(WakeNotice {
            token: first,
            task: TaskId::new(1),
            cause: WakeCause::Ready,
        });
    });
    let queued = signal.version();
    assert_ne!(queued, empty);
    hub.enqueue(WakeNotice {
        token: second,
        task: TaskId::new(2),
        cause: WakeCause::Ready,
    });
    assert_eq!(signal.version(), queued);
    assert!(hub.pop_wake().is_some());
    assert!(hub.pop_wake().is_some());
    hub.release();
    hub.release();
}
