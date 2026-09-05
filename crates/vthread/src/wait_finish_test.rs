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
    assert_eq!(cell.take_resource(), None);

    let cancelled = parked(&cell, &hub);
    assert!(cell.cancel());
    assert!(hub.pop_wake().is_some());
    assert_eq!(
        cell.finish_permit_ready(cancelled).unwrap(),
        WakeCause::Cancelled
    );
}

#[test]
fn permit_slow_path_consumes_the_ownership_marker() {
    let cell = WaitCell::new();
    let primary = Arc::new(WaitHub::new(1, Arc::default()));
    let alternate = Arc::new(WaitHub::new(1, Arc::default()));
    let first = parked(&cell, &primary);
    assert_eq!(cell.notify(), NotifyResult::Woke);
    assert!(primary.pop_wake().is_some());
    assert_eq!(cell.finish(first).unwrap(), WakeCause::Ready);

    let selected = parked(&cell, &alternate);
    assert!(cell.offer_resource(ResourceSelection::Permit));
    assert!(alternate.pop_wake().is_some());
    assert_eq!(
        cell.finish_permit_ready(selected).unwrap(),
        WakeCause::Ready
    );
    assert_eq!(cell.take_resource(), None);
}

#[test]
fn gate_close_reset_changes_only_the_closed_bit_of_an_idle_word() {
    use super::wait_state::{MAX_GENERATION, Phase, WaitWord};
    let cell = WaitCell::new();
    for generation in [0, 1, MAX_GENERATION] {
        for phase in 0..=12 {
            for closed in [false, true] {
                for permit in [false, true] {
                    for fallback in [false, true] {
                        for resource in [
                            None,
                            Some(ResourceSelection::Permit),
                            Some(ResourceSelection::Broadcast),
                        ] {
                            let word = WaitWord::from_raw(phase)
                                .with_generation(generation)
                                .with_closed(closed)
                                .with_permit(permit)
                                .with_fallback_hub(fallback)
                                .with_resource(resource);
                            cell.state.store(word);
                            let idle = word.phase() == Phase::Idle;
                            assert_eq!(cell.reset_closed_gate(), idle);
                            let expected = if idle { word.with_closed(false) } else { word };
                            assert_eq!(cell.state.load(), expected);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn late_selectors_cannot_reclose_or_wake_a_reused_gate_cell() {
    use std::{sync::Barrier, thread};
    for _ in 0..64 {
        let cell = WaitCell::new();
        let hub = Arc::new(WaitHub::new(1, Arc::default()));
        let first = parked(&cell, &hub);
        let old = cell.registration();
        assert!(cell.close());
        assert_eq!(hub.pop_wake().unwrap().token, first);
        assert_eq!(cell.finish(first).unwrap(), WakeCause::Closed);
        assert!(
            !cell.recycle(),
            "ordinary recycling must not reopen public pairs"
        );
        assert!(cell.reset_closed_gate());
        let next = parked(&cell, &hub);
        assert!(next.generation() > first.generation());
        let barrier = Barrier::new(5);
        thread::scope(|threads| {
            threads.spawn(|| {
                barrier.wait();
                assert!(!old.select_ready(first));
            });
            threads.spawn(|| {
                barrier.wait();
                assert!(!old.select_cancelled(first));
            });
            threads.spawn(|| {
                barrier.wait();
                assert!(!old.select_closed(first));
            });
            threads.spawn(|| {
                barrier.wait();
                assert!(!old.select_timeout(first).unwrap());
            });
            barrier.wait();
            assert_eq!(cell.notify(), NotifyResult::Woke);
        });
        assert_eq!(hub.pop_wake().unwrap().token, next);
        assert!(hub.pop_wake().is_none());
        assert_eq!(cell.finish(next).unwrap(), WakeCause::Ready);
        assert!(!cell.is_closed());
        assert!(cell.recycle());
    }
}
