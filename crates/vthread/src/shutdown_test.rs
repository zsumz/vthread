use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

use crate::support_test::until;
use crate::{CarrierStatus, Error, Runtime, TaskFailure, park_pair};

struct DropCount(Arc<AtomicUsize>);
impl Drop for DropCount {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn shutdown_reclaims_parked_children_before_join_returns_and_rejects_admission() {
    let runtime = Arc::new(Runtime::builder().carriers(2).build().expect("runtime"));
    let drops = Arc::new(AtomicUsize::new(0));
    runtime
        .scope(|scope| {
            let mut children = Vec::new();
            for _ in 0..2 {
                let (parker, _unparker) = park_pair();
                let flag = DropCount(Arc::clone(&drops));
                children.push(scope.spawn("parked", move || {
                    let _flag = flag;
                    parker.park().expect("park");
                })?);
            }
            until(|| scope.snapshot().parked == 2);
            let remote = Arc::clone(&runtime);
            thread::spawn(move || remote.shutdown())
                .join()
                .expect("shutdown thread")?;
            assert_eq!(drops.load(Ordering::SeqCst), 2);
            assert!(matches!(
                scope.spawn("late", || ()),
                Err(Error::RuntimeStopped)
            ));
            for child in children {
                assert!(matches!(
                    child.join(),
                    Err(Error::TaskAborted {
                        reason: TaskFailure::RuntimeStopped,
                        ..
                    })
                ));
            }
            assert_eq!(scope.snapshot().active, 0);
            Ok(())
        })
        .expect("observed failures");
    assert!(matches!(
        runtime.scope(|_| Ok(())),
        Err(Error::RuntimeStopped)
    ));
    assert!(
        runtime
            .snapshot()
            .carriers
            .iter()
            .all(|carrier| carrier.status == CarrierStatus::Stopped)
    );
    runtime.shutdown().expect("idempotent shutdown");
}

#[test]
fn racing_shutdown_and_spawn_drop_every_capture_once() {
    let runtime = Arc::new(Runtime::builder().carriers(2).build().expect("runtime"));
    let drops = Arc::new(AtomicUsize::new(0));
    runtime
        .scope(|scope| {
            let remote = Arc::clone(&runtime);
            let stopper = thread::spawn(move || remote.shutdown());
            let mut accepted = Vec::new();
            for _ in 0..128 {
                let flag = DropCount(Arc::clone(&drops));
                match scope.spawn("race", move || drop(flag)) {
                    Ok(child) => accepted.push(child),
                    Err(Error::RuntimeStopped) => {}
                    Err(error) => return Err(error),
                }
            }
            stopper.join().expect("stopper")?;
            for child in accepted {
                assert!(matches!(
                    child.join(),
                    Ok(())
                        | Err(Error::TaskAborted {
                            reason: TaskFailure::RuntimeStopped,
                            ..
                        })
                ));
            }
            assert_eq!(drops.load(Ordering::SeqCst), 128);
            assert_eq!(scope.snapshot().active, 0);
            Ok(())
        })
        .expect("scope");
}

#[test]
fn blocking_runtime_operations_are_rejected_inside_a_virtual_thread() {
    let runtime = Arc::new(Runtime::new().expect("runtime"));
    runtime
        .scope(|scope| {
            let nested = Arc::clone(&runtime);
            scope
                .spawn("nested", move || {
                    assert!(matches!(
                        nested.scope(|_| Ok(())),
                        Err(Error::InsideVThread)
                    ));
                    assert!(matches!(nested.shutdown(), Err(Error::InsideVThread)));
                })?
                .join()
        })
        .expect("scope");
}

#[test]
fn forced_stack_cleanup_keeps_runtime_calls_inside_the_virtual_thread_boundary() {
    struct CleanupProbe {
        runtime: Arc<Runtime>,
        rejected: Arc<AtomicUsize>,
    }
    impl Drop for CleanupProbe {
        fn drop(&mut self) {
            if matches!(self.runtime.scope(|_| Ok(())), Err(Error::InsideVThread)) {
                self.rejected.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
    let runtime = Arc::new(Runtime::new().expect("runtime"));
    let rejected = Arc::new(AtomicUsize::new(0));
    runtime
        .scope(|scope| {
            let probe = CleanupProbe {
                runtime: Arc::clone(&runtime),
                rejected: Arc::clone(&rejected),
            };
            let (parker, _unparker) = park_pair();
            let child = scope.spawn("cleanup boundary", move || {
                let _probe = probe;
                parker.park().expect("park");
            })?;
            until(|| scope.snapshot().parked == 1);
            runtime.shutdown()?;
            assert!(matches!(child.join(), Err(Error::TaskAborted { .. })));
            assert_eq!(rejected.load(Ordering::SeqCst), 1);
            Ok(())
        })
        .expect("scope");
}
