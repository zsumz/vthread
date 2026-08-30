use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use crate::{Error, Runtime, TaskStatus, yield_now};

#[test]
fn round_robin_mounts_are_visible() {
    let runtime = Runtime::new().expect("build runtime");
    let trace = Rc::new(RefCell::new(Vec::new()));

    runtime
        .scope(|scope| {
            let left_trace = Rc::clone(&trace);
            let left = scope.spawn("left", move || {
                left_trace.borrow_mut().push("left:1");
                yield_now().expect("mounted task");
                left_trace.borrow_mut().push("left:2");
                20
            })?;
            let right_trace = Rc::clone(&trace);
            let right = scope.spawn("right", move || {
                right_trace.borrow_mut().push("right:1");
                yield_now().expect("mounted task");
                right_trace.borrow_mut().push("right:2");
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

    assert_eq!(
        &*trace.borrow(),
        &["left:1", "right:1", "left:2", "right:2"]
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
    struct DropFlag(Rc<Cell<bool>>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    let runtime = Runtime::new().expect("build runtime");
    let dropped = Rc::new(Cell::new(false));
    let task_dropped = Rc::clone(&dropped);
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
    assert!(dropped.get());
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
