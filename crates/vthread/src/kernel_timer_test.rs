use std::{sync::Arc, sync::Mutex, time::Duration, time::Instant};

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

#[test]
fn sleeping_parks_instead_of_blocking_the_next_task() {
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().expect("scope");
    let trace = Arc::new(Mutex::new(Vec::new()));
    let sleeper_trace = Arc::clone(&trace);
    shared
        .submit(scope, "sleeper".into(), move || {
            sleeper_trace.lock().expect("trace").push("sleep:start");
            crate::sleep(Duration::from_secs(60)).expect("sleep task");
            sleeper_trace.lock().expect("trace").push("sleep:end");
        })
        .expect("submit sleeper");
    let worker_trace = Arc::clone(&trace);
    shared
        .submit(scope, "worker".into(), move || {
            worker_trace.lock().expect("trace").push("worker");
        })
        .expect("submit worker");

    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();
    assert!(kernel.tick(true).expect("park sleeper"));
    let parked = kernel
        .parked
        .iter()
        .next()
        .expect("sleep registered a park");
    let (route, token) = (parked.task, parked.token);
    assert!(kernel.timers.cancel(token));
    assert!(
        kernel
            .tick(false)
            .expect("run worker while sleep is pending")
    );
    assert_eq!(&*trace.lock().expect("trace"), &["sleep:start", "worker"]);

    // Drive the existing timer generation due only after the worker has run. Host preemption
    // must not decide whether a one-millisecond timer outranks an already-runnable task.
    assert!(kernel.timers.schedule(route, token, Instant::now()));
    assert!(kernel.tick(false).expect("expire and resume sleeper"));
    assert!(!kernel.tick(false).expect("drain completed tasks"));
    assert_eq!(
        &*trace.lock().expect("trace"),
        &["sleep:start", "worker", "sleep:end"]
    );
    assert_eq!(kernel.stats.parks, 1);
    assert_eq!(kernel.stats.timeouts, 1);
    shared.wait(scope, None).expect("scope succeeds");
    shared.finish_scope(scope);
}
