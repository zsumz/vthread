use std::{cell::RefCell, rc::Rc, time::Duration};

use crate::{ParkOutcome, Runtime, WakeReason, park_pair};

#[test]
fn timeout_updates_task_and_runtime_ledgers() {
    let runtime = Runtime::new().expect("build runtime");
    runtime
        .scope(|scope| {
            let (parker, _unparker) = park_pair();
            let task = scope.spawn("timer", move || {
                parker
                    .park_timeout(Duration::from_millis(1))
                    .expect("park with timeout")
            })?;
            assert_eq!(task.join()?, ParkOutcome::TimedOut);
            let snapshot = scope.snapshot();
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
    let trace = Rc::new(RefCell::new(Vec::new()));
    let (parker, unparker) = park_pair();
    runtime
        .scope(|scope| {
            let waiter_trace = Rc::clone(&trace);
            let waiter = scope.spawn("waiter", move || {
                waiter_trace.borrow_mut().push("before");
                parker.park().expect("park");
                waiter_trace.borrow_mut().push("after");
            })?;
            let wake_trace = Rc::clone(&trace);
            scope.spawn("wake", move || {
                wake_trace.borrow_mut().push("wake");
                unparker.unpark();
            })?;
            waiter.join()?;
            let waiter = scope
                .snapshot()
                .tasks
                .into_iter()
                .find(|task| task.name == "waiter")
                .expect("waiter snapshot");
            assert_eq!(waiter.mounts, 2);
            Ok(())
        })
        .expect("scope succeeds");
    assert_eq!(&*trace.borrow(), &["before", "wake", "after"]);
}
