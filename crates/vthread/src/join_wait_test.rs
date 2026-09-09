use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use std::time::Duration;

use crate::{
    Error, JoinHandle, Runtime, ScopeOptions, SuspensionReason, TaskStatus, UnparkResult,
    options::TaskOptions,
    park_pair,
    support_test::{run_isolated, until},
    task::{SharedTaskRecord, TaskCell, TaskRecord},
};

const UNSET: u8 = 0;
const REJECTED: u8 = 1;
const WAITED: u8 = 2;
const UNEXPECTED: u8 = 3;

struct JoinOnDrop<T> {
    handle: JoinHandle<T>,
    outcome: Arc<AtomicU8>,
}

struct DirectJoinOnDrop {
    record: SharedTaskRecord,
    outcome: Arc<AtomicU8>,
}

impl Drop for DirectJoinOnDrop {
    fn drop(&mut self) {
        let task = self.record.lock().id;
        let outcome = match super::wait_for(&self.record, SuspensionReason::Join(task), false) {
            Err(Error::SuspensionDuringPanic) => REJECTED,
            Ok(()) => WAITED,
            Err(_) => UNEXPECTED,
        };
        self.outcome.store(outcome, Ordering::SeqCst);
    }
}

fn unfinished_record_without_waiter_capacity() -> SharedTaskRecord {
    Arc::new(TaskCell::new(
        TaskRecord {
            id: crate::TaskId::new(u64::MAX),
            scope: u64::MAX,
            parent: None,
            options: Some(TaskOptions::root(ScopeOptions::default(), 1)),
            name: "unfinished completion".into(),
            carrier: crate::CarrierId(0),
            deadline: None,
            failure: None,
            status: TaskStatus::Queued,
            parks: 0,
            last_suspension: None,
            last_wake: None,
            outcome_observed: false,
            panic: None,
        },
        0,
    ))
}

impl<T> Drop for JoinOnDrop<T> {
    fn drop(&mut self) {
        let outcome = match self.handle.wait() {
            Err(Error::SuspensionDuringPanic) => REJECTED,
            Ok(()) => WAITED,
            Err(_) => UNEXPECTED,
        };
        self.outcome.store(outcome, Ordering::SeqCst);
    }
}

#[test]
fn a_virtual_join_parks_and_releases_the_single_carrier_for_other_work() {
    let runtime = Runtime::new().unwrap();
    runtime
        .run_scope(|scope| {
            let (parker, waker) = park_pair();
            let mut first = scope.spawn("target", move || {
                parker.park().unwrap();
                42
            })?;
            let mut joining = scope.spawn("joiner", move || first.join())?;
            until(|| scope.runtime_snapshot().parked == 2);
            let mut ready = scope.spawn("other work", move || waker.unpark())?;
            ready.join()?;
            assert_eq!(joining.join()??, 42);
            Ok(())
        })
        .unwrap();
}

#[test]
fn self_join_is_typed_misuse_without_corrupting_completion() {
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            assert_eq!(
                scope
                    .spawn("self wait", || {
                        let mounted = crate::context::current().unwrap();
                        let execution = mounted.execution().unwrap();
                        let id = execution.record().lock().id;
                        assert!(matches!(
                            super::wait_for(
                                execution.record(),
                                crate::SuspensionReason::Join(id),
                                false
                            ),
                            Err(crate::Error::JoinSelf)
                        ));
                        42
                    })?
                    .join()?,
                42
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn panic_is_rejected_before_waiting_for_an_unfinished_join() {
    const CHILD: &str = "VTHREAD_PANIC_JOIN_CHILD";
    const TEST: &str =
        "join_wait::join_wait_test::panic_is_rejected_before_waiting_for_an_unfinished_join";
    if std::env::var_os(CHILD).is_none() {
        let output = run_isolated(TEST, (CHILD, "1"), Duration::from_secs(15));
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && !output.timed_out && stdout.contains("1 passed"),
            "panic-join child failed: status={:?} timed_out={}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            output.status,
            output.timed_out,
        );
        return;
    }

    let runtime = Runtime::builder().carriers(1).build().unwrap();
    let outcome = Arc::new(AtomicU8::new(UNSET));
    runtime
        .run_scope(|scope| {
            let (parker, unparker) = park_pair();
            let target = scope.spawn("unfinished target", move || parker.park())?;
            until(|| scope.runtime_snapshot().parked == 1);

            let task_outcome = Arc::clone(&outcome);
            let mut panicking = scope.spawn("panic join", move || {
                let _join = JoinOnDrop {
                    handle: target,
                    outcome: task_outcome,
                };
                panic!("expected join panic");
            })?;
            assert!(matches!(panicking.join(), Err(Error::TaskPanicked { .. })));
            assert_eq!(unparker.unpark(), UnparkResult::Woke);
            Ok(())
        })
        .unwrap();

    assert_eq!(outcome.load(Ordering::SeqCst), REJECTED);
    assert_eq!(runtime.snapshot().active, 0);
    assert_eq!(runtime.snapshot().parked, 0);
    runtime.shutdown().unwrap();
}

#[test]
fn panic_join_is_rejected_before_completion_subscription() {
    let outcome = Arc::new(AtomicU8::new(UNSET));
    let task_outcome = Arc::clone(&outcome);
    let record = unfinished_record_without_waiter_capacity();
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            let mut task = scope.spawn("panic direct join", move || {
                let _join = DirectJoinOnDrop {
                    record,
                    outcome: task_outcome,
                };
                panic!("expected direct join panic");
            })?;
            assert!(matches!(task.join(), Err(Error::TaskPanicked { .. })));
            Ok(())
        })
        .unwrap();

    assert_eq!(outcome.load(Ordering::SeqCst), REJECTED);
}
