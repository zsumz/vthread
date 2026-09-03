use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use crate::{CarrierId, RuntimeConfig, TaskId, control::Shared, kernel::Kernel};

use super::{current, mount};

#[test]
fn mounted_execution_hot_state_fits_with_its_rc_header_in_one_cache_line() {
    assert_eq!(std::mem::size_of::<super::Execution>(), 48);
}

#[test]
fn mounts_restore_the_previous_task() {
    assert!(current().is_none());
    {
        let _outer = mount(TaskId::new(1));
        assert_eq!(current().expect("outer task").task_id(), TaskId::new(1));
        {
            let _inner = mount(TaskId::new(2));
            assert_eq!(current().expect("inner task").task_id(), TaskId::new(2));
        }
        assert_eq!(current().expect("outer restored").task_id(), TaskId::new(1));
    }
    assert!(current().is_none());
}

#[test]
fn dispatch_mounts_execution_context_and_restores_cleanup_overrides() {
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().expect("scope");
    let expected = Arc::new(AtomicU64::new(0));
    let body_expected = Arc::clone(&expected);
    let spawned = shared
        .submit(scope, "task".into(), move || {
            let expected = TaskId::new(body_expected.load(Ordering::Relaxed));
            assert_eq!(current().expect("running task").task_id(), expected);
            {
                let _cleanup = mount(TaskId::new(u64::MAX));
                assert_eq!(
                    current().expect("cleanup override").task_id(),
                    TaskId::new(u64::MAX)
                );
            }
            assert_eq!(current().expect("restored task").task_id(), expected);
            crate::yield_now().expect("yield");
            assert_eq!(current().expect("resumed task").task_id(), expected);
        })
        .expect("task");
    expected.store(spawned.id.get(), Ordering::Relaxed);
    let mut kernel = Kernel::new(shared, CarrierId(0));
    kernel.receive();

    assert!(kernel.tick(false).expect("initial dispatch"));
    assert!(current().is_none());
    assert!(kernel.tick(false).expect("resumed dispatch"));
    assert!(current().is_none());
    assert!(!kernel.tick(false).expect("idle kernel"));
}

#[test]
fn one_execution_reuses_its_lazy_readiness_wait() {
    let runtime = crate::Runtime::new().unwrap();
    runtime
        .run_scope(|scope| {
            scope
                .spawn("readiness-wait", || {
                    let mounted = current().unwrap();
                    let execution = mounted.execution().unwrap();
                    let first = execution.readiness_parker().unwrap();
                    let identity = first.wait.identity();
                    drop(first);
                    assert_eq!(
                        execution.readiness_parker().unwrap().wait.identity(),
                        identity
                    );
                })?
                .join()
        })
        .unwrap();
}
