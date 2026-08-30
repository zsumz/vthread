use std::{
    rc::{Rc, Weak},
    time::{Duration, Instant},
};

use crate::{Error, TaskId};

use super::{NotifyResult, WaitBegin, WaitCell, WaitHub, WakeCause};

fn parked(cell: &WaitCell, hub: &Rc<WaitHub>) -> vthread_stack::ParkToken {
    match cell
        .begin(
            TaskId::new(1),
            hub,
            Some(Instant::now() + Duration::from_secs(1)),
        )
        .expect("begin wait")
    {
        WaitBegin::Park(request) => request.token(),
        WaitBegin::Immediate(cause) => panic!("unexpected immediate wake: {cause:?}"),
    }
}

#[test]
fn one_preexisting_permit_is_bounded() {
    let cell = WaitCell::new();
    let hub = Rc::new(WaitHub::new());
    assert_eq!(cell.notify(), NotifyResult::Stored);
    assert_eq!(cell.notify(), NotifyResult::Stored);
    assert!(matches!(
        cell.begin(TaskId::new(1), &hub, None)
            .expect("consume permit"),
        WaitBegin::Immediate(WakeCause::Ready)
    ));
    assert!(matches!(
        cell.begin(TaskId::new(1), &hub, None).expect("next wait"),
        WaitBegin::Park(_)
    ));
}

#[test]
fn duplicate_registration_does_not_replace_the_original_wait() {
    let cell = WaitCell::new();
    let hub = Rc::new(WaitHub::new());
    let token = parked(&cell, &hub);

    let duplicate = hub
        .register(token, Weak::new())
        .expect_err("duplicate token must be rejected");
    assert!(matches!(
        duplicate,
        Error::Invariant("wait token registered twice")
    ));

    let registration = hub.take_registration(token).expect("original registration");
    assert!(
        registration
            .select_timeout(token)
            .expect("original state remains")
    );
    assert_eq!(
        hub.pop_wake().expect("timeout wake").cause,
        WakeCause::TimedOut
    );
}

#[test]
fn timeout_wins_one_generation_exactly_once() {
    let cell = WaitCell::new();
    let hub = Rc::new(WaitHub::new());
    let token = parked(&cell, &hub);
    let registration = hub.take_registration(token).expect("registration");

    assert!(registration.select_timeout(token).expect("select timeout"));
    assert!(
        !registration
            .select_timeout(token)
            .expect("duplicate timeout")
    );
    assert_eq!(cell.notify(), NotifyResult::Stored);
    assert_eq!(hub.pop_wake().expect("one wake").cause, WakeCause::TimedOut);
    assert!(hub.pop_wake().is_none());
    assert_eq!(
        cell.finish(token).expect("finish wait"),
        WakeCause::TimedOut
    );
    assert!(matches!(
        cell.begin(TaskId::new(1), &hub, None)
            .expect("stored next permit"),
        WaitBegin::Immediate(WakeCause::Ready)
    ));
}

#[test]
fn stale_generation_cannot_select_a_later_wait() {
    let cell = WaitCell::new();
    let hub = Rc::new(WaitHub::new());
    let first = parked(&cell, &hub);
    let first_registration = hub.take_registration(first).expect("first registration");
    assert_eq!(cell.notify(), NotifyResult::Woke);
    assert_eq!(hub.pop_wake().expect("ready wake").cause, WakeCause::Ready);
    assert_eq!(cell.finish(first).expect("finish first"), WakeCause::Ready);

    let second = parked(&cell, &hub);
    assert_ne!(first.generation(), second.generation());
    assert!(
        !first_registration
            .select_timeout(first)
            .expect("stale timeout is harmless")
    );
    assert!(hub.pop_wake().is_none());
    cell.rollback(second);
}

#[test]
fn cancellation_and_close_are_distinct_winners() {
    let cell = WaitCell::new();
    let hub = Rc::new(WaitHub::new());
    let first = parked(&cell, &hub);
    let _registration = hub.take_registration(first).expect("registration");
    assert!(cell.cancel());
    assert_eq!(
        hub.pop_wake().expect("cancel wake").cause,
        WakeCause::Cancelled
    );
    assert_eq!(
        cell.finish(first).expect("finish cancel"),
        WakeCause::Cancelled
    );

    let second = parked(&cell, &hub);
    let _registration = hub.take_registration(second).expect("registration");
    assert!(cell.close());
    assert!(!cell.close());
    assert_eq!(hub.pop_wake().expect("close wake").cause, WakeCause::Closed);
    assert_eq!(
        cell.finish(second).expect("finish close"),
        WakeCause::Closed
    );
    assert!(cell.is_closed());
}
