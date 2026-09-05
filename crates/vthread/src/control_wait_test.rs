use crate::{Error, Runtime, park_pair};
use std::time::Duration;

#[test]
fn disabled_stall_detection_drains_from_scope_activity_without_record_locks() {
    use std::sync::{Arc, mpsc};

    let config = Runtime::builder().build().unwrap().config();
    let shared = Arc::new(super::Shared::new(config));
    let scope = shared.begin_scope().unwrap();
    let record = shared.reserve(scope, "terminal".into(), None).unwrap();
    shared.complete(&record, None);
    let record_guard = record.lock();
    let observer = Arc::clone(&shared);
    let (sent, received) = mpsc::sync_channel(1);
    let waiter = std::thread::spawn(move || sent.send(observer.wait(scope, None)));

    let result = received.recv_timeout(Duration::from_secs(1));
    drop(record_guard);
    waiter.join().unwrap().unwrap();
    assert!(
        result
            .expect("scope drain touched a terminal record")
            .is_ok()
    );
    shared.finish_scope(scope);
}

#[test]
fn terminal_records_awaiting_credit_retirement_are_not_stalled() {
    use crate::{StallPolicy, control::CompletionBatch, support_test::until};
    use std::sync::Arc;

    for policy in [
        StallPolicy::ReportAfter(Duration::ZERO),
        StallPolicy::AbortAfter(Duration::ZERO),
    ] {
        let config = Runtime::builder()
            .stall_policy(policy)
            .build()
            .unwrap()
            .config();
        let shared = Arc::new(super::Shared::new(config));
        let scope = shared.begin_scope().unwrap();
        let record = shared.reserve(scope, "terminal".into(), None).unwrap();
        let mut batch = CompletionBatch::new();
        batch.push(shared.prepare_completion(&record, None).unwrap());
        let observer = Arc::clone(&shared);
        let waiter = std::thread::spawn(move || observer.wait(scope, None));

        // Hold the legitimate terminal-to-retirement gap open until the owner waits.
        until(|| shared.changed.waiting() != 0);
        let abort = shared.abort_reason(scope);
        let stall = shared.snapshot().last_stall;
        record.completion().complete();
        shared.publish_completions(&batch, &shared.scope_progress(scope));
        let result = waiter.join().unwrap();
        shared.finish_scope(scope);

        assert!(
            abort.is_none(),
            "terminal-only scope was aborted: {policy:?}"
        );
        assert!(
            stall.is_none(),
            "terminal-only scope was reported: {policy:?}"
        );
        assert!(result.is_ok(), "terminal-only scope failed: {result:?}");
    }
}

#[test]
fn a_pending_wake_for_another_scope_does_not_mask_a_stall() {
    use crate::{
        ScopeOptions, SuspensionReason, TaskFailure, TaskStatus,
        task_slab::TaskKey,
        wait::{WakeCause, WakeNotice},
    };
    use std::{sync::Arc, time::Instant};
    let config = Runtime::builder()
        .stall_policy(crate::StallPolicy::AbortAfter(Duration::from_millis(10)))
        .build()
        .unwrap()
        .config();
    let shared = Arc::new(super::Shared::new(config));
    let scope = shared.begin_scope().unwrap();
    let record = shared.reserve(scope, "ownerless".into(), None).unwrap();
    shared.transition(&record, |r| {
        r.status = TaskStatus::Suspended(SuspensionReason::Park)
    });
    let other_scope = shared.begin_owned(ScopeOptions::default(), true).unwrap();
    let other = shared.reserve(other_scope, "other".into(), None).unwrap();
    let token = vthread_stack::ParkToken::new(100, 1);
    let hub = &shared.inboxes[0].hub;
    hub.enqueue(WakeNotice {
        token,
        task: other.lock().id,
        route: TaskKey::owned(0),
        cause: WakeCause::Ready,
    });
    let observer = Arc::clone(&shared);
    let watched = Arc::clone(&record);
    let watchdog = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_millis(200);
        while observer.abort_reason(scope).is_none() && Instant::now() < deadline {
            std::thread::yield_now();
        }
        let reason = observer.abort_reason(scope);
        observer.complete(&watched, reason);
    });
    let result = shared.wait(scope, None);
    watchdog.join().unwrap();
    shared.complete(&other, Some(TaskFailure::SupervisorStopped));
    assert!(matches!(
        result.as_ref().map_err(crate::Error::primary),
        Err(Error::RuntimeStalled { active: 1 })
    ));
}

#[test]
fn unrelated_supervisor_activity_cannot_hide_a_stalled_root_scope() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let runtime = Runtime::builder()
        .carriers(2)
        .stall_policy(crate::StallPolicy::AbortAfter(Duration::from_millis(10)))
        .build()
        .unwrap();
    let supervisor = runtime
        .supervisor_with(crate::ScopeOptions::default())
        .unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let mut busy = supervisor
        .spawn("unrelated", move || {
            while !worker_stop.load(Ordering::Acquire) {
                crate::yield_now().unwrap();
            }
        })
        .unwrap();
    let (parker, wake) = park_pair();
    let watchdog = std::thread::spawn(move || {
        std::thread::park_timeout(Duration::from_millis(200));
        wake.unpark();
    });
    let result = runtime.run_scope(|scope| {
        let _ = scope.spawn("ownerless", move || parker.park())?;
        Ok(())
    });
    stop.store(true, Ordering::Release);
    busy.join().unwrap();
    supervisor.shutdown().unwrap();
    watchdog.join().unwrap();
    assert!(matches!(
        result.as_ref().map_err(crate::Error::primary),
        Err(Error::RuntimeStalled { active: 1 })
    ));
    let stall = runtime.snapshot().last_stall.unwrap();
    assert_eq!(stall.tasks.len(), 1);
    assert_eq!(stall.tasks[0].name, "ownerless");
}

#[test]
fn a_terminal_sibling_does_not_hide_an_indefinitely_parked_child() {
    let runtime = Runtime::builder()
        .carriers(2)
        .stall_policy(crate::StallPolicy::AbortAfter(Duration::from_millis(10)))
        .build()
        .expect("runtime");
    let (parker, _unparker) = park_pair();
    let error = runtime
        .run_scope(|scope| {
            let _ = scope.spawn("parked", move || parker.park())?;
            scope.spawn("terminal", || 42)?.join()?;
            Ok(())
        })
        .expect_err("parked child must be reclaimed");
    assert!(matches!(
        error.primary(),
        Error::RuntimeStalled { active: 1 }
    ));
    assert_eq!(runtime.snapshot().active, 0);
    runtime
        .run_scope(|scope| scope.spawn("reused", || ())?.join())
        .expect("reusable");
}
