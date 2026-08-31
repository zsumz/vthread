use crate::{Error, Runtime, RuntimeConfig, control::Shared, support_test::until};
use std::sync::{
    Arc, Barrier,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

#[test]
fn close_and_reserve_have_one_order_and_reserved_work_prevents_drain() {
    for _ in 0..64 {
        let shared = Arc::new(Shared::new(RuntimeConfig::default()));
        let scope = shared.begin_scope().unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let remote = Arc::clone(&shared);
        let entered = Arc::clone(&barrier);
        let reserve = std::thread::spawn(move || {
            entered.wait();
            remote.reserve(scope, "racing admission".into(), None)
        });
        barrier.wait();
        shared.close_scope(scope);
        match reserve.join().unwrap() {
            Ok(record) => {
                assert!(
                    !shared
                        .wait_until(scope, None, Some(Instant::now()))
                        .unwrap()
                );
                shared.complete(&record, None);
            }
            Err(Error::ScopeClosed) => {}
            Err(error) => panic!("unexpected admission error: {error}"),
        }
        assert!(matches!(
            shared.reserve(scope, "late".into(), None),
            Err(Error::ScopeClosed)
        ));
        assert!(
            shared
                .wait_until(scope, None, Some(Instant::now()))
                .unwrap()
        );
        shared.finish_scope(scope);
        assert_eq!(shared.snapshot().active(), 0);
    }
}

#[test]
fn dropping_a_dynamic_handle_keeps_reclamation_owned_by_the_root() {
    struct Guard(Arc<AtomicBool>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let runtime = Runtime::new().unwrap();
    let dropped = Arc::new(AtomicBool::new(false));
    let guard = Guard(Arc::clone(&dropped));
    runtime
        .run_scope(|scope| {
            let spawner = scope.spawner();
            scope
                .spawn("parent", move || {
                    drop(spawner.spawn("owned child", move || {
                        let _guard = guard;
                        crate::yield_now()
                    })?);
                    Ok::<_, Error>(())
                })?
                .join()??;
            Ok(())
        })
        .unwrap();
    assert!(dropped.load(Ordering::SeqCst));
    assert_eq!(runtime.snapshot().active(), 0);
}

#[test]
fn root_unwind_closes_escaped_capabilities_before_reclamation() {
    let runtime = Runtime::new().unwrap();
    let mut retained = None;
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _: crate::Result<()> = runtime.run_scope(|scope| {
            retained = Some(scope.spawner());
            panic!("root body");
        });
    }));
    assert!(panic.is_err());
    assert!(matches!(
        retained.unwrap().spawn("late", || ()),
        Err(Error::ScopeClosed)
    ));
    runtime.run_scope(|_| Ok(())).unwrap();
}

#[test]
fn remote_capabilities_close_on_supervisor_stop_and_rejection_drops_outside_locks() {
    struct Reenter(crate::Spawner, Arc<AtomicBool>);
    impl Drop for Reenter {
        fn drop(&mut self) {
            assert!(matches!(
                self.0.spawn("reentrant", || ()),
                Err(Error::ScopeClosed)
            ));
            self.1.store(true, Ordering::SeqCst);
        }
    }
    let runtime = Runtime::new().unwrap();
    let supervisor = runtime.supervisor().unwrap();
    let spawner = supervisor.spawner();
    let remote = spawner.clone();
    assert_eq!(
        std::thread::spawn(move || remote.spawn("OS child", || 7).unwrap().join().unwrap())
            .join()
            .unwrap(),
        7
    );
    supervisor.request_shutdown();
    let dropped = Arc::new(AtomicBool::new(false));
    let capture = Reenter(spawner.clone(), Arc::clone(&dropped));
    assert!(matches!(
        spawner.spawn("closed", move || drop(capture)),
        Err(Error::ScopeClosed)
    ));
    assert!(dropped.load(Ordering::SeqCst));
    supervisor.shutdown().unwrap();
    assert!(matches!(
        spawner.spawn("finished", || ()),
        Err(Error::ScopeClosed)
    ));
}

#[test]
fn already_admitted_parent_can_create_borrowed_children_during_root_drain() {
    let runtime = Runtime::new().unwrap();
    let (release, gate) = mpsc::sync_channel(1);
    let finished = Arc::new(AtomicBool::new(false));
    runtime
        .run_scope(|scope| {
            let done = Arc::clone(&finished);
            drop(scope.spawn("admitted parent", move || {
                gate.recv_timeout(Duration::from_secs(5)).unwrap();
                crate::local_scope(|local| {
                    let value = local.spawn("borrowed child", || 42)?.join()?;
                    assert_eq!(value, 42);
                    done.store(true, Ordering::SeqCst);
                    Ok(())
                })
                .unwrap();
            })?);
            *crate::signal::lock(&runtime.shared.scope_drain_hook) = Some(Box::new(move || {
                release.send(()).unwrap();
            }));
            Ok(())
        })
        .unwrap();
    assert!(finished.load(Ordering::SeqCst));
}

#[test]
fn dynamic_capacity_rejection_keeps_existing_children_owned() {
    let runtime = Runtime::builder()
        .max_vthreads(2)
        .stack_cache_capacity(0)
        .build()
        .unwrap();
    runtime
        .run_scope(|scope| {
            let spawner = scope.spawner();
            scope
                .spawn("parent", move || {
                    let mut child =
                        spawner.spawn("last slot", || crate::sleep(Duration::from_secs(5)))?;
                    assert!(matches!(
                        spawner.spawn("excess", || ()),
                        Err(Error::Capacity {
                            resource: crate::error::CapacityResource::Tasks,
                            limit: 2
                        })
                    ));
                    child.cancel();
                    assert!(matches!(child.join()?, Err(Error::Cancelled)));
                    assert_eq!(spawner.spawn("replacement", || 42)?.join()?, 42);
                    Ok::<_, Error>(())
                })?
                .join()??;
            until(|| runtime.snapshot().active() == 0);
            Ok(())
        })
        .unwrap();
}
