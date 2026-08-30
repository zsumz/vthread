use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use crate::{Error, ParkOutcome, Runtime, TaskStatus, park_pair, yield_now};

#[test]
fn round_robin_mounts_are_visible() {
    let runtime = Runtime::new().expect("build runtime");
    let trace = Arc::new(Mutex::new(Vec::new()));

    runtime
        .scope(|scope| {
            let left_trace = Arc::clone(&trace);
            let left = scope.spawn("left", move || {
                left_trace.lock().expect("trace").push("left:1");
                yield_now().expect("mounted task");
                left_trace.lock().expect("trace").push("left:2");
                20
            })?;
            let right_trace = Arc::clone(&trace);
            let right = scope.spawn("right", move || {
                right_trace.lock().expect("trace").push("right:1");
                yield_now().expect("mounted task");
                right_trace.lock().expect("trace").push("right:2");
                22
            })?;

            assert_eq!(left.join()? + right.join()?, 42);
            let snapshot = scope.snapshot();
            assert_eq!(snapshot.stats.mounts, 4);
            assert_eq!(snapshot.stats.yields, 2);
            assert!(snapshot.tasks.iter().all(|task| task.mounts == 2));
            assert!(
                snapshot
                    .tasks
                    .iter()
                    .all(|task| task.last_suspension == Some(crate::SuspensionReason::YieldNow))
            );
            assert!(
                snapshot
                    .tasks
                    .iter()
                    .all(|task| task.status == TaskStatus::Completed)
            );
            Ok(())
        })
        .expect("scope succeeds");

    let trace = trace.lock().expect("trace");
    assert!(
        trace.iter().position(|entry| *entry == "left:1")
            < trace.iter().position(|entry| *entry == "left:2")
    );
    assert!(
        trace.iter().position(|entry| *entry == "right:1")
            < trace.iter().position(|entry| *entry == "right:2")
    );
}

#[test]
fn a_task_panic_does_not_stop_the_carrier() {
    let runtime = Runtime::new().expect("build runtime");
    runtime
        .scope(|scope| {
            let failed = scope.spawn("failed", || panic!("boom"))?;
            let healthy = scope.spawn("healthy", || 42)?;
            let error = failed.join().expect_err("panic becomes join error");
            assert!(matches!(error, Error::TaskPanicked { .. }));
            assert_eq!(healthy.join()?, 42);
            Ok(())
        })
        .expect("observed panic does not fail scope");
}

#[test]
fn panic_runs_stack_destructors() {
    struct DropFlag(Arc<AtomicBool>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let runtime = Runtime::new().expect("build runtime");
    let dropped = Arc::new(AtomicBool::new(false));
    let task_dropped = Arc::clone(&dropped);
    runtime
        .scope(|scope| {
            let failed = scope.spawn("failed", move || {
                let _flag = DropFlag(task_dropped);
                panic!("boom");
            })?;
            assert!(failed.join().is_err());
            Ok(())
        })
        .expect("panic is observed");
    assert!(dropped.load(Ordering::SeqCst));
}

#[test]
fn completed_stacks_are_reused_by_later_tasks() {
    let runtime = Runtime::builder()
        .stack_cache_capacity(1)
        .build()
        .expect("build runtime");
    runtime
        .scope(|scope| {
            scope.spawn("first", || ())?.join()?;
            scope.spawn("second", || ())?.join()?;
            Ok(())
        })
        .expect("scope succeeds");

    let stacks = runtime.snapshot().stacks;
    assert_eq!(stacks.allocated, 1);
    assert_eq!(stacks.reused, 1);
}

#[test]
fn a_stalled_parked_scope_is_cleaned_before_reuse() {
    let runtime = Runtime::builder()
        .stall_timeout(Some(Duration::from_millis(10)))
        .build()
        .expect("build runtime");
    let (parker, _unparker) = park_pair();
    let parker = Arc::new(parker);
    let parked_parker = Arc::clone(&parker);
    let error = runtime
        .scope(|scope| {
            let _parked = scope.spawn("parked", move || parked_parker.park())?;
            Ok(())
        })
        .expect_err("an unowned indefinite park must stall");
    assert!(matches!(error, Error::RuntimeStalled { active: 1 }));

    let snapshot = runtime.snapshot();
    assert_eq!(snapshot.active, 0);
    assert_eq!(snapshot.parked, 0);
    assert_eq!(snapshot.timers, 0);
    assert_eq!(snapshot.stats.aborted, 1);

    runtime
        .scope(|scope| {
            assert_eq!(scope.spawn("reused", || 42)?.join()?, 42);
            let reused_parker = Arc::clone(&parker);
            let parked = scope.spawn("park-again", move || {
                reused_parker.park_timeout(Duration::from_millis(1))
            })?;
            assert_eq!(parked.join()??, ParkOutcome::TimedOut);
            Ok(())
        })
        .expect("runtime and parker remain reusable");
}
#[test]
fn an_unjoined_result_destructor_panic_does_not_kill_the_carrier() {
    struct BadDrop;
    impl Drop for BadDrop {
        fn drop(&mut self) {
            panic!("result destructor");
        }
    }
    let runtime = crate::Runtime::new().unwrap();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let result = runtime.scope(|scope| {
        drop(scope.spawn("unjoined", move || {
            rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
            BadDrop
        })?);
        tx.send(()).unwrap();
        Ok(())
    });
    assert!(matches!(result, Err(crate::Error::TaskPanicked { .. })));
    runtime
        .scope(|scope| {
            assert_eq!(scope.spawn("still alive", || 42)?.join()?, 42);
            Ok(())
        })
        .unwrap();
}
