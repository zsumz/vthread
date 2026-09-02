use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{ParkOutcome, Runtime, WakeReason, park_pair};

#[test]
fn wake_processing_follows_the_observed_signal_epoch() {
    use crate::{CarrierId, RuntimeConfig, control::Shared, kernel::Kernel};

    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().expect("scope");
    let (parker, waker) = park_pair();
    shared
        .submit(scope, "signalled".into(), move || parker.park())
        .expect("submit");
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();
    assert!(kernel.tick(true).expect("park task"));

    waker.unpark();
    assert_eq!(kernel.inbox.hub.pending(), 1);
    assert!(!kernel.tick(false).expect("unchanged signal"));
    assert_eq!(kernel.inbox.hub.pending(), 1);

    assert!(kernel.tick(true).expect("changed signal"));
    assert_eq!(kernel.inbox.hub.pending(), 0);
    shared.finish_scope(scope);
}

#[test]
fn timeout_updates_task_and_runtime_ledgers() {
    let runtime = Runtime::new().expect("build runtime");
    runtime
        .run_scope(|scope| {
            let (parker, _unparker) = park_pair();
            let mut task = scope.spawn("timer", move || {
                parker
                    .park_timeout(Duration::from_millis(1))
                    .expect("park with timeout")
            })?;
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
fn parked_tasks_do_not_consume_mounts_until_selected() {
    let runtime = Runtime::new().expect("build runtime");
    let trace = Arc::new(Mutex::new(Vec::new()));
    let (parker, unparker) = park_pair();
    runtime
        .run_scope(|scope| {
            let waiter_trace = Arc::clone(&trace);
            let mut waiter = scope.spawn("waiter", move || {
                waiter_trace.lock().expect("trace").push("before");
                parker.park().expect("park");
                waiter_trace.lock().expect("trace").push("after");
            })?;
            crate::support_test::until(|| scope.runtime_snapshot().parked == 1);
            let wake_trace = Arc::clone(&trace);
            let _ = scope.spawn("wake", move || {
                wake_trace.lock().expect("trace").push("wake");
                unparker.unpark();
            })?;
            waiter.join()?;
            let waiter = scope
                .runtime_snapshot()
                .tasks
                .into_iter()
                .find(|task| task.name == "waiter")
                .expect("waiter snapshot");
            assert_eq!(waiter.mounts, 2);
            Ok(())
        })
        .expect("scope succeeds");
    assert_eq!(&*trace.lock().expect("trace"), &["before", "wake", "after"]);
}
#[test]
fn reclaiming_a_selected_but_unresumed_park_releases_the_active_generation() {
    use crate::{
        CarrierId, RuntimeConfig, TaskFailure, TaskId, control::Shared, kernel::Kernel,
        wait::WaitBegin,
    };
    use std::sync::Arc;
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().unwrap();
    let (parker, waker) = crate::park_pair();
    let parker = Arc::new(parker);
    let child_parker = Arc::clone(&parker);
    shared
        .submit(scope, "selected".into(), move || child_parker.park())
        .unwrap();
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();
    kernel.tick(true).unwrap();
    waker.unpark();
    kernel.process_wakes().unwrap();
    kernel.abort(None, TaskFailure::RuntimeStopped);
    let next = parker
        .wait
        .begin(TaskId::new(99), &kernel.inbox.hub, None)
        .expect("old generation reclaimed");
    if let WaitBegin::Park(request) = next {
        parker.wait.rollback(request.token());
    } else {
        panic!("expected a fresh generation");
    }
}
