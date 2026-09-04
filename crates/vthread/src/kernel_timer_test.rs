use std::{sync::Arc, time::Duration, time::Instant};

use crate::{
    CarrierId, ParkOutcome, Runtime, RuntimeConfig, WakeReason, control::Shared, kernel::Kernel,
    park_pair, parking::UnparkResult, task_slab::TaskKey,
};

#[test]
fn timeout_updates_task_and_runtime_ledgers() {
    let runtime = Runtime::new().expect("build runtime");
    runtime
        .run_scope(|scope| {
            let (parker, _unparker) = park_pair();
            let mut task = scope.spawn("timer", move || {
                parker
                    .park_timeout(Duration::from_secs(1))
                    .expect("park with timeout")
            })?;
            crate::support_test::until(|| scope.runtime_snapshot().parked == 1);
            assert_eq!(task.join()?, ParkOutcome::TimedOut);
            let snapshot = scope.runtime_snapshot();
            let task = snapshot
                .tasks
                .iter()
                .find(|task| task.name == "timer")
                .expect("task");
            assert_eq!(task.parks, 1);
            assert_eq!(task.last_wake, Some(WakeReason::TimedOut));
            assert_eq!(snapshot.stats.wakes, 1);
            assert_eq!(snapshot.stats.timeouts, 1);
            assert_eq!(snapshot.parked, 0);
            assert_eq!(snapshot.timers, 0);
            Ok(())
        })
        .expect("scope succeeds");
}

#[test]
fn expired_timer_for_a_stale_route_cannot_select_a_live_wait() {
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().expect("scope");
    let (parker, waker) = park_pair();
    shared
        .submit(scope, "parked".into(), move || {
            parker.park_timeout(Duration::from_secs(60))
        })
        .expect("submit");
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();
    assert!(kernel.tick(true).expect("park task"));

    let parked = kernel.parked.iter().next().expect("parked task");
    let token = parked.token;
    let route = parked.task;
    assert!(kernel.timers.cancel(token));
    let stale_route = if route.is_borrowed() {
        TaskKey::owned(route.index())
    } else {
        TaskKey::borrowed(route.index())
    };
    assert!(kernel.timers.schedule(stale_route, token, Instant::now()));

    assert!(!kernel.tick(false).expect("ignore stale timer route"));
    assert_eq!(kernel.parked.len(), 1);
    assert_eq!(kernel.timers.active_count(), 0);

    assert_eq!(waker.unpark(), UnparkResult::Woke);
    assert!(kernel.tick(true).expect("resume exact wait"));
    assert!(!kernel.tick(false).expect("drained kernel"));
    shared.finish_scope(scope);
}
