use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{ParkOutcome, Runtime, WakeReason, park_pair};

#[test]
fn first_mount_publication_lag_is_bounded_by_one_batch() {
    use crate::{CarrierId, control::Shared, kernel::Kernel};

    let config = Runtime::builder()
        .max_vthreads(64)
        .carrier_queue_capacity(64)
        .build()
        .expect("config")
        .config();
    let shared = Arc::new(Shared::new(config));
    let scope = shared.begin_scope().expect("scope");
    for _ in 0..64 {
        shared
            .submit(scope, "yield once".into(), crate::yield_now)
            .expect("submit");
    }
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();

    for _ in 0..63 {
        assert!(kernel.tick(true).expect("first mount"));
    }
    assert_eq!(shared.snapshot().stats.mounts, 0);
    assert!(kernel.tick(true).expect("publication boundary"));
    assert_eq!(shared.snapshot().stats.mounts, 64);
    kernel.abort(None, crate::TaskFailure::RuntimeStopped);
}

#[test]
fn a_lone_first_mount_is_published_before_user_code_runs() {
    use crate::{CarrierId, RuntimeConfig, control::Shared, kernel::Kernel};

    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().expect("scope");
    let observer = Arc::clone(&shared);
    shared
        .submit(scope, "lone".into(), move || {
            assert_eq!(observer.snapshot().stats.mounts, 1);
        })
        .expect("submit");
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();

    assert!(kernel.tick(true).expect("complete task"));
    assert!(shared.wait(scope, None).is_ok());
    shared.finish_scope(scope);
}

#[test]
fn bulk_completion_publication_lag_is_bounded_and_the_last_task_flushes() {
    use crate::{CarrierId, control::Shared, kernel::Kernel};

    let config = Runtime::builder()
        .max_vthreads(64)
        .carrier_queue_capacity(64)
        .build()
        .expect("config")
        .config();
    let shared = Arc::new(Shared::new(config));
    let scope = shared.begin_scope().expect("scope");
    for _ in 0..64 {
        shared
            .submit(scope, "complete".into(), || ())
            .expect("submit");
    }
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();

    for _ in 0..31 {
        assert!(kernel.tick(true).expect("complete task"));
    }
    assert_eq!(shared.snapshot().stats.completed, 0);
    assert!(kernel.tick(true).expect("publication boundary"));
    assert_eq!(shared.snapshot().stats.completed, 32);
    while kernel.tick(true).expect("drain") {}
    assert_eq!(shared.snapshot().stats.completed, 64);
}

#[test]
fn pending_wakes_are_processed_without_a_signal_epoch_change() {
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
    assert!(kernel.tick(false).expect("pending wake"));
    assert_eq!(kernel.inbox.hub.pending(), 0);
    assert!(!kernel.tick(false).expect("drained wake"));
    shared.finish_scope(scope);
}

#[test]
fn same_carrier_ready_wakes_bypass_the_shared_inbox() {
    use crate::{CarrierId, RuntimeConfig, control::Shared, kernel::Kernel};

    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().expect("scope");
    let (parker, waker) = park_pair();
    shared
        .submit(scope, "parked".into(), move || parker.park())
        .expect("parked task");
    shared
        .submit(scope, "wake".into(), move || waker.unpark())
        .expect("wake task");
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();

    assert!(kernel.tick(true).expect("park task"));
    assert!(kernel.tick(false).expect("wake task"));
    assert_eq!(kernel.local.pending_wakes(), 1);
    assert_eq!(kernel.inbox.hub.pending(), 0);
    assert!(kernel.tick(false).expect("resume task"));
    assert_eq!(kernel.local.pending_wakes(), 0);
    assert!(!kernel.tick(false).expect("drained kernel"));
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
