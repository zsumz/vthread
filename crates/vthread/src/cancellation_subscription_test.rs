use super::CancellationToken;
use crate::{TaskId, task_slab::TaskKey, wait::WaitBegin, wait::WaitCell, wait::WaitHub};
use std::sync::{Arc, Barrier};

#[test]
fn active_subscription_does_not_retain_duplicate_node_ownership() {
    let token = CancellationToken::root(1);
    let owners = Arc::strong_count(&token.0);
    let hub = Arc::new(WaitHub::new(1, Arc::default()));
    let wait = WaitCell::new();
    let WaitBegin::Park {
        request,
        registration,
    } = wait
        .begin(TaskId::new(1), TaskKey::owned(0), &hub, None)
        .unwrap()
    else {
        panic!("expected active wait");
    };

    let subscription = token.register(request.token(), &registration).unwrap();
    assert_eq!(Arc::strong_count(&token.0), owners);

    drop(subscription);
    wait.rollback(request.token());
}

#[test]
fn cancellation_racing_subscription_publication_selects_every_generation() {
    for _ in 0..128 {
        let cancellation = CancellationToken::root(1);
        let hub = Arc::new(WaitHub::new(1, Arc::default()));
        let wait = WaitCell::new();
        let WaitBegin::Park {
            request,
            registration,
        } = wait
            .begin(TaskId::new(1), TaskKey::owned(0), &hub, None)
            .unwrap()
        else {
            panic!("expected active wait");
        };
        let barrier = Arc::new(Barrier::new(2));

        std::thread::scope(|threads| {
            let remote_barrier = Arc::clone(&barrier);
            let remote_cancellation = &cancellation;
            let remote = threads.spawn(move || {
                remote_barrier.wait();
                remote_cancellation.cancel();
            });
            barrier.wait();
            let subscription = cancellation
                .register(request.token(), &registration)
                .unwrap();
            remote.join().unwrap();

            assert_eq!(hub.pending(), 1);
            assert_eq!(hub.pop_wake().unwrap().token, request.token());
            assert!(hub.pop_wake().is_none());
            assert_eq!(
                wait.finish(request.token()).unwrap(),
                crate::wait::WakeCause::InheritedCancelled
            );
            drop(subscription);
        });
    }
}

#[test]
fn resident_wait_registration_coexists_with_a_shared_primary() {
    let cancellation = CancellationToken::root(1);
    let hub = Arc::new(WaitHub::new(2, Arc::default()));
    let shared = WaitCell::new();
    let WaitBegin::Park {
        request,
        registration,
    } = shared
        .begin(TaskId::new(1), TaskKey::owned(0), &hub, None)
        .unwrap()
    else {
        panic!("expected shared wait");
    };
    drop(
        cancellation
            .register(request.token(), &registration)
            .unwrap(),
    );
    shared.rollback(request.token());

    let resident = WaitCell::new();
    let WaitBegin::Park { request, .. } = resident
        .begin_resident(TaskId::new(1), TaskKey::owned(0), &hub, None)
        .unwrap()
    else {
        panic!("expected resident wait");
    };
    let subscription = cancellation
        .register_resident(request.token(), &resident)
        .unwrap();
    cancellation.cancel();

    assert_eq!(hub.pending(), 1);
    assert_eq!(hub.pop_wake().unwrap().token, request.token());
    assert!(hub.pop_wake().is_none());
    assert_eq!(
        resident.finish(request.token()).unwrap(),
        crate::wait::WakeCause::InheritedCancelled
    );
    drop(subscription);
}
