use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{TaskId, task_slab::TaskKey};

use super::{NotifyResult, WaitBegin, WaitCell, WaitHub, WakeCause};

fn parked(cell: &WaitCell, hub: &Arc<WaitHub>) -> vthread_stack::ParkToken {
    match cell
        .begin(
            TaskId::new(1),
            TaskKey::owned(0),
            hub,
            Some(Instant::now() + Duration::from_secs(1)),
        )
        .expect("begin wait")
    {
        WaitBegin::Park { request, .. } => request.token(),
        WaitBegin::Immediate(cause) => panic!("unexpected immediate wake: {cause:?}"),
    }
}

#[test]
fn one_preexisting_permit_is_bounded() {
    let cell = WaitCell::new();
    let hub = Arc::new(WaitHub::new(64, Arc::default()));
    assert_eq!(cell.notify(), NotifyResult::Stored);
    assert_eq!(cell.notify(), NotifyResult::Stored);
    assert!(matches!(
        cell.begin(TaskId::new(1), TaskKey::owned(0), &hub, None)
            .expect("consume permit"),
        WaitBegin::Immediate(WakeCause::Ready)
    ));
    assert!(matches!(
        cell.begin(TaskId::new(1), TaskKey::owned(0), &hub, None)
            .expect("next wait"),
        WaitBegin::Park { .. }
    ));
}

#[test]
fn successful_generation_does_not_construct_runtime_faults() {
    let before = crate::error::RuntimeFault::created_on_current_thread();
    let cell = WaitCell::new();
    let hub = Arc::new(WaitHub::new(1, Arc::default()));
    let WaitBegin::Park { request, .. } = cell
        .begin(TaskId::new(1), TaskKey::owned(0), &hub, None)
        .expect("begin")
    else {
        panic!("expected park");
    };
    assert_eq!(cell.notify(), NotifyResult::Woke);
    assert!(hub.pop_wake().is_some());
    assert_eq!(cell.finish(request.token()).unwrap(), WakeCause::Ready);
    assert_eq!(
        crate::error::RuntimeFault::created_on_current_thread(),
        before
    );
}

#[test]
fn notification_finish_fast_path_preserves_non_ready_winners() {
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
fn completed_generations_retain_one_reusable_owner_hub() {
    let cell = WaitCell::new();
    let hub = Arc::new(WaitHub::new(1, Arc::default()));
    let owners = Arc::strong_count(&hub);
    let WaitBegin::Park { request, .. } = cell
        .begin(TaskId::new(1), TaskKey::owned(0), &hub, None)
        .unwrap()
    else {
        panic!("expected park");
    };
    assert_eq!(cell.notify(), NotifyResult::Woke);
    assert!(hub.pop_wake().is_some());
    assert_eq!(cell.finish(request.token()).unwrap(), WakeCause::Ready);
    assert_eq!(Arc::strong_count(&hub), owners + 1);
}

#[test]
fn recycled_waits_discard_internal_permits() {
    let cell = WaitCell::new();
    let hub = Arc::new(WaitHub::new(1, Arc::default()));
    assert_eq!(cell.notify(), NotifyResult::Stored);
    assert!(cell.recycle());
    let WaitBegin::Park { request, .. } = cell
        .begin(TaskId::new(1), TaskKey::owned(0), &hub, None)
        .unwrap()
    else {
        panic!("recycled permit leaked into the next owner");
    };
    cell.rollback(request.token());
}

#[test]
fn dropping_a_wait_releases_its_cached_owner_hub() {
    let hub = Arc::new(WaitHub::new(1, Arc::default()));
    let owners = Arc::strong_count(&hub);
    let cell = WaitCell::new();
    let WaitBegin::Park { .. } = cell
        .begin(TaskId::new(1), TaskKey::owned(0), &hub, None)
        .unwrap()
    else {
        panic!("expected an active wait");
    };
    assert_eq!(Arc::strong_count(&hub), owners + 1);
    drop(cell);
    assert_eq!(Arc::strong_count(&hub), owners);
}

#[test]
fn registration_selects_the_original_wait() {
    let cell = WaitCell::new();
    let hub = Arc::new(WaitHub::new(64, Arc::default()));
    let token = parked(&cell, &hub);

    let registration = cell.registration();
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
    let hub = Arc::new(WaitHub::new(64, Arc::default()));
    let token = parked(&cell, &hub);
    let registration = cell.registration();

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
        cell.begin(TaskId::new(1), TaskKey::owned(0), &hub, None)
            .expect("stored next permit"),
        WaitBegin::Immediate(WakeCause::Ready)
    ));
}

#[test]
fn stale_generation_cannot_select_a_later_wait() {
    let cell = WaitCell::new();
    let hub = Arc::new(WaitHub::new(64, Arc::default()));
    let first = parked(&cell, &hub);
    let first_registration = cell.registration();
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
    let hub = Arc::new(WaitHub::new(64, Arc::default()));
    let first = parked(&cell, &hub);
    let _registration = cell.registration();
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
    let _registration = cell.registration();
    assert!(cell.close());
    assert!(!cell.close());
    assert_eq!(hub.pop_wake().expect("close wake").cause, WakeCause::Closed);
    assert_eq!(
        cell.finish(second).expect("finish close"),
        WakeCause::Closed
    );
    assert!(cell.is_closed());
}

#[test]
fn reused_waits_route_each_generation_to_its_current_owner() {
    let cell = WaitCell::new();
    let primary = Arc::new(WaitHub::new(2, Arc::default()));
    let alternate = Arc::new(WaitHub::new(2, Arc::default()));
    for (task, route, hub) in [
        (TaskId::new(1), TaskKey::owned(0), &primary),
        (TaskId::new(2), TaskKey::owned(1), &primary),
        (TaskId::new(3), TaskKey::borrowed(0), &alternate),
    ] {
        let WaitBegin::Park { request, .. } = cell.begin(task, route, hub, None).unwrap() else {
            panic!("expected park");
        };
        assert_eq!(cell.notify(), NotifyResult::Woke);
        let notice = hub.pop_wake().expect("current owner wake");
        assert_eq!((notice.task, notice.route), (task, route));
        assert_eq!(cell.finish(request.token()).unwrap(), WakeCause::Ready);
    }
}
