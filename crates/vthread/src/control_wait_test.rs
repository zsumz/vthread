use crate::{Error, Runtime, park_pair};
use std::time::Duration;

#[test]
fn a_pending_wake_for_another_scope_does_not_mask_a_stall() {
    use crate::{
        ScopeOptions, SuspensionReason, TaskFailure, TaskStatus,
        signal::lock,
        wait::{WakeCause, WakeNotice},
    };
    use std::{
        sync::{Arc, Weak},
        time::Instant,
    };
    let config = Runtime::builder()
        .stall_timeout(Some(Duration::from_millis(10)))
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
    hub.register(token, Weak::new()).unwrap();
    hub.enqueue(WakeNotice {
        token,
        task: lock(&other).id,
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
    assert!(matches!(result, Err(Error::RuntimeStalled { active: 1 })));
}

#[test]
fn unrelated_supervisor_activity_cannot_hide_a_stalled_root_scope() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let runtime = Runtime::builder()
        .carriers(2)
        .stall_timeout(Some(Duration::from_millis(10)))
        .build()
        .unwrap();
    let supervisor = runtime.supervisor(crate::ScopeOptions::default()).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let busy = supervisor
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
    let result = runtime.scope(|scope| {
        scope.spawn("ownerless", move || parker.park())?;
        Ok(())
    });
    stop.store(true, Ordering::Release);
    busy.join().unwrap();
    supervisor.shutdown().unwrap();
    watchdog.join().unwrap();
    assert!(matches!(result, Err(Error::RuntimeStalled { active: 1 })));
    let stall = runtime.snapshot().last_stall.unwrap();
    assert_eq!(stall.tasks.len(), 1);
    assert_eq!(stall.tasks[0].name, "ownerless");
}

#[test]
fn a_terminal_sibling_does_not_hide_an_indefinitely_parked_child() {
    let runtime = Runtime::builder()
        .carriers(2)
        .stall_timeout(Some(Duration::from_millis(10)))
        .build()
        .expect("runtime");
    let (parker, _unparker) = park_pair();
    let error = runtime
        .scope(|scope| {
            scope.spawn("parked", move || parker.park())?;
            scope.spawn("terminal", || 42)?.join()?;
            Ok(())
        })
        .expect_err("parked child must be reclaimed");
    assert!(matches!(error, Error::RuntimeStalled { active: 1 }));
    assert_eq!(runtime.snapshot().active, 0);
    runtime
        .scope(|scope| scope.spawn("reused", || ())?.join())
        .expect("reusable");
}
