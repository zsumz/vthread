use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use crate::{
    CarrierId, ParkOutcome, Runtime, TaskFailure, UnparkResult,
    control::Shared,
    kernel::Kernel,
    park_pair,
    task_slab::TaskKey,
    wait::{WakeCause, WakeNotice},
};

#[test]
fn reused_task_routes_reject_stale_wait_generations() {
    for target in [0, 31] {
        let config = Runtime::builder()
            // Completed records remain scope-owned; leave room for the new
            // record while verifying reuse of the carrier's physical task slot.
            .max_vthreads(64)
            .carrier_queue_capacity(64)
            .build()
            .unwrap()
            .config();
        let shared = Arc::new(Shared::new(config));
        let scope = shared.begin_scope().unwrap();
        let resumed = Arc::new(AtomicUsize::new(0));
        let mut parkers = Vec::new();
        let mut wakers = Vec::new();
        for index in 0..32 {
            let (parker, waker) = park_pair();
            let parker = Arc::new(parker);
            parkers.push(Arc::clone(&parker));
            wakers.push(waker);
            let resumed = Arc::clone(&resumed);
            shared
                .submit(scope, "route owner".into(), move || {
                    for _ in 0..if index == target { 2 } else { 1 } {
                        assert_eq!(parker.park().unwrap(), ParkOutcome::Ready);
                        resumed.fetch_add(1, Ordering::Relaxed);
                    }
                })
                .unwrap();
        }
        let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
        for _ in 0..32 {
            kernel.receive();
            assert!(kernel.tick(false).unwrap());
        }
        let route = TaskKey::owned(target);
        let old_token = kernel.parked.get(route).unwrap().token;
        let old_task = kernel.task(route).execution().id;
        let old_registration = parkers[target].wait.registration();
        assert_eq!(old_token.wait(), parkers[target].wait.identity());

        assert_eq!(wakers[target].unpark(), UnparkResult::Woke);
        assert!(kernel.tick(false).unwrap());
        assert_eq!(resumed.load(Ordering::Relaxed), 1);
        let next_token = kernel.parked.get(route).unwrap().token;
        assert_ne!(next_token, old_token);
        assert!(!old_registration.select_ready(old_token));
        kernel.inbox.hub.enqueue(WakeNotice {
            token: old_token,
            task: old_task,
            route,
            cause: WakeCause::Ready,
        });
        assert!(!kernel.tick(false).unwrap());
        assert_eq!(kernel.parked.get(route).unwrap().token, next_token);
        assert_eq!(kernel.stats.stale_wakes, 1);
        assert_eq!(resumed.load(Ordering::Relaxed), 1);

        assert_eq!(wakers[target].unpark(), UnparkResult::Woke);
        assert!(kernel.tick(false).unwrap());
        assert!(!kernel.tick(false).unwrap());
        assert_eq!(resumed.load(Ordering::Relaxed), 2);

        let parker = Arc::clone(&parkers[target]);
        let next_resumed = Arc::clone(&resumed);
        shared
            .submit(scope, "recycled route".into(), move || {
                assert_eq!(parker.park().unwrap(), ParkOutcome::Ready);
                next_resumed.fetch_add(1, Ordering::Relaxed);
            })
            .unwrap();
        kernel.receive();
        assert!(kernel.tick(false).unwrap());
        assert_ne!(kernel.task(route).execution().id, old_task);
        assert!(!old_registration.select_ready(old_token));
        assert!(!old_registration.select_ready(next_token));
        assert_eq!(kernel.inbox.hub.pending(), 0);
        assert!(!kernel.tick(false).unwrap());
        assert_eq!(resumed.load(Ordering::Relaxed), 2);
        assert_eq!(wakers[target].unpark(), UnparkResult::Woke);
        assert!(kernel.tick(false).unwrap());
        assert_eq!(resumed.load(Ordering::Relaxed), 3);
        kernel.abort(None, TaskFailure::RuntimeStopped);
        shared.finish_scope(scope);
    }
}
