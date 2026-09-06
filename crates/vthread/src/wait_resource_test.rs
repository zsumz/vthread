use super::*;
use crate::{
    TaskId,
    task_slab::TaskKey,
    wait::{WaitBegin, WaitHub},
};
use std::sync::Arc;

#[test]
fn resource_consumption_cannot_mutate_binding_or_claimed_words() {
    for phase in [
        Phase::Binding,
        Phase::ClaimReady,
        Phase::ClaimTimedOut,
        Phase::ClaimCancelled,
        Phase::ClaimInheritedCancelled,
        Phase::ClaimClosed,
    ] {
        for resource in [ResourceSelection::Permit, ResourceSelection::Broadcast] {
            let cell = WaitCell::new();
            let exclusive = WaitWord::initial()
                .with_generation(41)
                .with_resource(Some(resource))
                .with_phase(phase);
            cell.state.store(exclusive);
            assert_eq!(cell.try_take_resource(), None, "mutated {phase:?}");
            assert_eq!(cell.state.load(), exclusive);
        }
    }
}

#[test]
fn a_reserved_generation_rejects_competitors_and_routes_only_on_publication() {
    let cell = WaitCell::new();
    let hub = Arc::new(WaitHub::new(1, Arc::default()));
    let WaitBegin::Park {
        request,
        registration,
    } = cell
        .begin(TaskId::new(1), TaskKey::owned(0), &hub, None)
        .unwrap()
    else {
        panic!("expected a park")
    };
    let publication = cell
        .clone()
        .reserve_resource(ResourceSelection::Permit)
        .unwrap();
    assert_eq!(cell.state.load().phase(), Phase::ClaimReady);
    assert!(hub.pop_wake().is_none());
    assert!(!cell.cancel());
    assert!(!registration.select_cancelled(request.token()));
    assert!(!registration.select_timeout(request.token()).unwrap());
    assert!(!registration.select_closed(request.token()));
    assert!(!cell.offer_resource(ResourceSelection::Broadcast));
    assert_eq!(cell.try_take_resource(), None);
    publication.publish();
    assert_eq!(hub.pop_wake().unwrap().token, request.token());
    assert!(hub.pop_wake().is_none());
    assert_eq!(
        cell.finish_permit_ready(request.token()).unwrap(),
        WakeCause::Ready
    );
    assert_eq!(cell.take_resource(), None);
}

#[test]
fn unwinding_a_deferred_publication_delivers_exactly_one_wake() {
    let cell = WaitCell::new();
    let hub = Arc::new(WaitHub::new(1, Arc::default()));
    let WaitBegin::Park { request, .. } = cell
        .begin(TaskId::new(1), TaskKey::owned(0), &hub, None)
        .unwrap()
    else {
        panic!("expected a park")
    };
    let publication = cell
        .clone()
        .reserve_resource(ResourceSelection::Permit)
        .unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _publication = publication;
            panic!("publisher unwinds before routing");
        }))
        .is_err()
    );
    assert_eq!(cell.state.load().phase(), Phase::SelectedReady);
    assert_eq!(hub.pop_wake().unwrap().token, request.token());
    assert!(hub.pop_wake().is_none());
    assert_eq!(
        cell.finish_permit_ready(request.token()).unwrap(),
        WakeCause::Ready
    );
}

#[test]
fn an_idle_grant_can_be_consumed_before_the_publication_guard_drops() {
    let cell = WaitCell::new();
    let publication = cell
        .clone()
        .reserve_resource(ResourceSelection::Permit)
        .unwrap();
    assert_eq!(cell.take_resource(), Some(ResourceSelection::Permit));
    assert_eq!(cell.take_resource(), None);
    assert!(cell.recycle());
    let recycled = cell.state.load();
    drop(publication);
    assert_eq!(cell.state.load(), recycled);
}

#[cfg(feature = "handoff-profiling")]
#[test]
fn mutex_owner_identity_is_not_shared_routing_and_ignores_inactive_targets() {
    use crate::{handoff_mutex::Grant, local_carrier::LocalCarrier};
    let local = std::rc::Rc::new(LocalCarrier::new(crate::RuntimeConfig::default()));
    let owner = Arc::new(WaitHub::new(2, Arc::default()));
    let foreign = Arc::new(WaitHub::new(2, Arc::default()));
    let cell = WaitCell::new();
    // An unbound stored grant must not attempt to read a target at all.
    let stored = cell
        .clone()
        .reserve_resource(ResourceSelection::Permit)
        .unwrap();
    assert!(matches!(stored.mutex_grant(), Some(Grant::Stored)));
    assert_eq!(cell.take_resource(), Some(ResourceSelection::Permit));
    assert!(cell.recycle());
    drop(stored);
    let _route = crate::context::mount_carrier(&owner, &local);
    // Reusing the cell across these hubs also exercises the fallback target.
    for hub in [&owner, &foreign, &owner] {
        let WaitBegin::Park { request, .. } = cell
            .begin(TaskId::new(1), TaskKey::owned(0), hub, None)
            .unwrap()
        else {
            panic!("expected active wait")
        };
        let mut publication = cell
            .clone()
            .reserve_resource(ResourceSelection::Permit)
            .unwrap();
        let grant = publication.mutex_grant().unwrap();
        assert_eq!(matches!(grant, Grant::SameOwner), Arc::ptr_eq(hub, &owner));
        assert_eq!(
            matches!(grant, Grant::OtherOwner),
            !Arc::ptr_eq(hub, &owner)
        );
        // Route via the shared hub even for the matching owner: queue selection
        // is deliberately not the identity oracle. Keep the exact claimed word.
        let claimed = publication.claimed.take().unwrap();
        enqueue_selected(&cell.state, claimed, WakeCause::Ready, None, false);
        drop(publication);
        assert!(local.pop_wake().is_none());
        assert_eq!(hub.pop_wake().unwrap().token, request.token());
        cell.finish_permit_ready(request.token()).unwrap();
        assert!(cell.recycle());
    }
    assert_eq!(local.handoff_profile.borrow().remote_publications(), 3);
    assert_eq!(local.handoff_profile.borrow().local_publications(), 0);
}
