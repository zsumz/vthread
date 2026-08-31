use crate::{Error, Runtime, ScopeOptions, TaskFailure, park_pair, support_test::until};

#[test]
fn supervised_work_survives_lexical_scopes_and_is_reclaimed_explicitly() {
    let runtime = Runtime::builder().carriers(2).build().unwrap();
    let supervisor = runtime.supervisor(ScopeOptions::default()).unwrap();
    let (parker, _waker) = park_pair();
    let mut child = supervisor.spawn("service", move || parker.park()).unwrap();
    until(|| runtime.snapshot().parked == 1);
    runtime
        .run_scope(|scope| {
            assert_eq!(scope.spawn("request", || 42)?.join()?, 42);
            Ok(())
        })
        .unwrap();
    assert!(!child.is_finished());
    let report = supervisor.shutdown().unwrap();
    assert_eq!(report.aborted, 1);
    assert!(matches!(
        child.join(),
        Err(Error::TaskAborted {
            reason: TaskFailure::SupervisorStopped,
            ..
        })
    ));
    assert_eq!(runtime.snapshot().active, 0);
}

#[test]
fn dropping_a_supervisor_never_detaches_its_children() {
    let runtime = Runtime::new().unwrap();
    let supervisor = runtime.supervisor(ScopeOptions::default()).unwrap();
    let (parker, _waker) = park_pair();
    let task = supervisor.spawn("owned", move || parker.park()).unwrap();
    until(|| runtime.snapshot().parked == 1);
    drop(supervisor);
    assert!(task.is_finished());
    assert_eq!(runtime.snapshot().active, 0);
    assert_eq!(runtime.shutdown().unwrap().aborted, 1);
}

#[test]
fn supervisor_shutdown_is_visible_to_cooperative_checkpoint_loops() {
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };
    let runtime = Runtime::new().unwrap();
    let supervisor = runtime.supervisor(ScopeOptions::default()).unwrap();
    let (tx, rx) = mpsc::sync_channel(1);
    let mut child = supervisor
        .spawn("cooperative", move || {
            tx.send(()).unwrap();
            let bound = Instant::now() + Duration::from_secs(5);
            while crate::checkpoint().is_ok() {
                assert!(
                    Instant::now() < bound,
                    "shutdown cancellation was not visible"
                );
                std::hint::spin_loop();
            }
        })
        .unwrap();
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    supervisor.shutdown().unwrap();
    child.join().unwrap();
}
#[test]
fn timed_shutdown_retains_supervised_work_until_retry_completes() {
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };
    let runtime = crate::Runtime::new().unwrap();
    let mut supervisor = runtime.supervisor(crate::ScopeOptions::default()).unwrap();
    let (release, gate) = mpsc::sync_channel(1);
    let (started, entered) = mpsc::sync_channel(1);
    let mut child = supervisor
        .spawn("uncooperative", move || {
            started.send(()).unwrap();
            gate.recv_timeout(Duration::from_secs(5)).unwrap();
        })
        .unwrap();
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let observed = supervisor
        .shutdown_until(Instant::now() + Duration::from_millis(20))
        .unwrap();
    let still_owned = supervisor.scope.is_some();
    release.send(()).unwrap();
    assert!(matches!(
        observed,
        crate::SupervisorShutdownOutcome::TimedOut(_)
    ));
    assert!(still_owned);
    assert!(matches!(
        supervisor.spawn("rejected", || ()),
        Err(crate::Error::RuntimeStopped)
    ));
    let finished = supervisor
        .shutdown_until(Instant::now() + Duration::from_secs(5))
        .unwrap();
    assert!(matches!(
        finished,
        crate::SupervisorShutdownOutcome::Complete(_)
    ));
    let crate::SupervisorShutdownOutcome::Complete(report) = finished else {
        panic!("completed owner");
    };
    let crate::SupervisorShutdownOutcome::Complete(repeated) =
        supervisor.shutdown_until(Instant::now()).unwrap()
    else {
        panic!("owner regressed");
    };
    assert_eq!(report, repeated);
    assert!(child.is_finished());
    assert!(supervisor.cancellation_token().is_cancelled());
    let _ = child.join();
    supervisor.shutdown().unwrap();
}
