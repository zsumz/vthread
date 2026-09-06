use super::*;
use crate::wait::WaitHub;
use crate::{
    TaskId,
    task_slab::TaskKey,
    wait::{WaitBegin, WakeCause},
};
use std::sync::Arc;

#[test]
fn claimed_owner_interest_does_not_become_a_next_generation_permit() {
    let signal = Arc::default();
    let hub = Arc::new(WaitHub::new(1, Arc::clone(&signal)));
    let cell = WaitCell::new();
    let WaitBegin::Park { request, .. } = cell
        .begin(TaskId::new(1), TaskKey::owned(0), &hub, None)
        .unwrap()
    else {
        panic!("expected park");
    };
    let token = request.token();
    let active = cell.state.load();
    let claim = active.claimed(WakeCause::Ready);
    assert!(cell.state.compare_exchange(active, claim).is_ok());
    let observed = signal.version();
    assert_eq!(cell.publication(token), Publication::InFlight);
    assert!(!cell.try_abandon(token));
    assert!(cell.state.load().has_permit());
    cell.state.publish_claim(claim);
    assert_ne!(signal.version(), observed);
    assert_eq!(cell.publication(token), Publication::Published);
    assert!(!cell.state.load().has_permit());
    assert_eq!(cell.finish(token).unwrap(), WakeCause::Ready);
    assert_eq!(cell.publication(token), Publication::Stale);
    assert!(matches!(
        cell.begin(TaskId::new(1), TaskKey::owned(0), &hub, None)
            .unwrap(),
        WaitBegin::Park { .. }
    ));
}

#[test]
fn owner_retirement_preserves_selected_resources_until_ticket_cleanup() {
    let hub = Arc::new(WaitHub::new(1, Arc::default()));
    let cell = WaitCell::new();
    let WaitBegin::Park { request, .. } = cell
        .begin(TaskId::new(1), TaskKey::owned(0), &hub, None)
        .unwrap()
    else {
        panic!("expected park");
    };
    let token = request.token();
    assert!(cell.offer_resource(super::super::ResourceSelection::Permit));
    assert!(cell.try_abandon(token));
    assert_eq!(cell.publication(token), Publication::Stale);
    assert_eq!(
        cell.take_resource(),
        Some(super::super::ResourceSelection::Permit)
    );
    assert_eq!(cell.take_resource(), None);
    assert!(hub.pop_wake().is_none());
}

#[test]
fn a_delayed_completion_notifies_the_original_hub_after_rebinding() {
    use crate::wait::wait_publication_probe_test::{PausedPublication, Stage};
    let original_signal = Arc::default();
    let next_signal = Arc::default();
    let original = Arc::new(WaitHub::new(1, Arc::clone(&original_signal)));
    let next = Arc::new(WaitHub::new(1, Arc::clone(&next_signal)));
    let cell = WaitCell::new();
    let WaitBegin::Park { request, .. } = cell
        .begin(TaskId::new(1), TaskKey::owned(0), &original, None)
        .unwrap()
    else {
        panic!("expected park");
    };
    let token = request.token();
    let active = cell.state.load();
    let claimed = active.claimed(WakeCause::Ready);
    assert!(cell.state.compare_exchange(active, claimed).is_ok());
    assert_eq!(cell.publication(token), Publication::InFlight);
    let original_epoch = original_signal.version();
    let next_epoch = next_signal.version();
    std::thread::scope(|threads| {
        let mut pause = PausedPublication::install_at(&cell, Stage::ClaimPublished);
        let publisher = threads.spawn(|| cell.state.publish_claim(claimed));
        pause.observe(Stage::ClaimPublished);
        assert_eq!(cell.finish(token).unwrap(), WakeCause::Ready);
        let WaitBegin::Park { request, .. } = cell
            .begin(TaskId::new(2), TaskKey::owned(0), &next, None)
            .unwrap()
        else {
            panic!("expected next park");
        };
        assert_ne!(request.token(), token);
        assert_eq!(cell.publication(token), Publication::Stale);
        assert_eq!(cell.notify(), crate::wait::NotifyResult::Woke);
        assert_eq!(next.pop_wake().unwrap().token, request.token());
        assert_eq!(cell.finish(request.token()).unwrap(), WakeCause::Ready);
        assert_eq!(original_signal.version(), original_epoch);
        pause.release();
        publisher.join().unwrap();
    });
    assert_ne!(original_signal.version(), original_epoch);
    assert_eq!(next_signal.version(), next_epoch);
}
