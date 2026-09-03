use crate::{RuntimeConfig, ScopeOptions, control::Shared};
use std::{
    sync::{Arc, mpsc},
    time::Duration,
};

#[test]
fn a_supervisor_does_not_take_the_lexical_scope_slot() {
    let shared = Shared::new(RuntimeConfig::default());
    let supervisor = shared.begin_owned(ScopeOptions::default(), true).unwrap();
    let lexical = shared.begin_scope().unwrap();
    shared.finish_scope(supervisor);
    assert!(shared.begin_scope().is_err());
    shared.finish_scope(lexical);
    assert!(shared.begin_scope().is_ok());
}

#[test]
fn scope_records_and_tasks_have_independent_uses_of_the_same_bound() {
    let runtime = crate::Runtime::builder()
        .max_vthreads(1)
        .stack_cache_capacity(0)
        .build()
        .unwrap();
    let supervisor = runtime.supervisor_with(ScopeOptions::default()).unwrap();
    assert!(matches!(
        runtime.supervisor_with(ScopeOptions::default()),
        Err(crate::Error::Capacity {
            resource: crate::error::CapacityResource::Scopes,
            limit: 1
        })
    ));
    assert_eq!(
        supervisor
            .spawn("independent task capacity", || 42)
            .unwrap()
            .join()
            .unwrap(),
        42
    );
    supervisor.shutdown().unwrap();
    runtime
        .run_scope(|scope| scope.spawn("replacement scope", || ())?.join())
        .unwrap();
}

#[test]
fn nested_local_groups_reuse_owned_scope_records_but_children_consume_task_capacity() {
    let runtime = crate::Runtime::builder()
        .max_vthreads(2)
        .stack_cache_capacity(0)
        .build()
        .unwrap();
    let supervisor = runtime.supervisor_with(ScopeOptions::default()).unwrap();
    runtime
        .run_scope(|scope| {
            let shared = std::sync::Arc::clone(&runtime.shared);
            scope
                .spawn("local parent", move || {
                    let mounted = crate::context::current().unwrap();
                    let parent = mounted.execution().unwrap();
                    let owned = parent.record().lock().scope;
                    crate::local_scope(|_| {
                        crate::local_scope(|local| {
                            assert_eq!(crate::signal::lock(&shared.state).scopes.len(), 2);
                            let mut child = local.spawn("local child", || 42)?;
                            assert_eq!(child.record.lock().scope, owned);
                            assert_eq!(crate::signal::lock(&shared.state).scopes.len(), 2);
                            assert!(matches!(
                                local.spawn("over task limit", || ()),
                                Err(crate::Error::Capacity {
                                    resource: crate::error::CapacityResource::Tasks,
                                    limit: 2
                                })
                            ));
                            assert_eq!(child.join()?, 42);
                            Ok(())
                        })
                    })
                    .unwrap();
                })?
                .join()
        })
        .unwrap();
    supervisor.shutdown().unwrap();
}

#[test]
fn an_explicit_owned_scope_budget_is_independent_of_task_capacity() {
    let runtime = crate::Runtime::builder()
        .max_vthreads(2)
        .max_owned_scopes(1)
        .stack_cache_capacity(0)
        .build()
        .unwrap();
    let supervisor = runtime.supervisor().unwrap();
    assert!(matches!(
        runtime.supervisor(),
        Err(crate::Error::Capacity {
            resource: crate::error::CapacityResource::Scopes,
            limit: 1
        })
    ));
    let mut left = supervisor.spawn("one", || 1).unwrap();
    let mut right = supervisor.spawn("two", || 2).unwrap();
    assert!(matches!(
        supervisor.spawn("excess", || ()),
        Err(crate::Error::Capacity {
            resource: crate::error::CapacityResource::Tasks,
            limit: 2
        })
    ));
    assert_eq!(left.join().unwrap() + right.join().unwrap(), 3);
    supervisor.shutdown().unwrap();
    runtime.run_scope(|_| Ok(())).unwrap();
}

#[test]
fn finishing_a_scope_does_not_relock_each_terminal_record() {
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().unwrap();
    let record = shared.reserve(scope, "terminal".into(), None).unwrap();
    shared.complete(&record, None);
    let record_guard = record.lock();
    let finishing = Arc::clone(&shared);
    let (sent, received) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        finishing.finish_scope(scope);
        sent.send(()).unwrap();
    });

    received
        .recv_timeout(Duration::from_secs(1))
        .expect("scope reclamation relocked a terminal record");
    drop(record_guard);
    worker.join().unwrap();
}

#[test]
fn successful_scope_failure_collection_does_not_lock_terminal_records() {
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().unwrap();
    let record = shared.reserve(scope, "successful".into(), None).unwrap();
    shared.complete(&record, None);
    let record_guard = record.lock();
    let observing = Arc::clone(&shared);
    let (sent, received) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        sent.send(observing.unobserved(scope).failure_count())
            .unwrap();
    });

    let failure_count = received
        .recv_timeout(Duration::from_secs(1))
        .expect("successful failure collection locked a terminal record");
    assert_eq!(failure_count, 0);
    drop(record_guard);
    worker.join().unwrap();
    shared.finish_scope(scope);
}

#[test]
fn uniquely_owned_terminal_task_cells_are_reset_and_reused() {
    let config = crate::Runtime::builder()
        .max_vthreads(1)
        .stack_cache_capacity(1)
        .build()
        .unwrap()
        .config();
    let shared = Shared::new(config);
    let first_scope = shared.begin_scope().unwrap();
    let first = shared.reserve(first_scope, "first".into(), None).unwrap();
    let address = Arc::as_ptr(&first);
    shared.complete(&first, None);
    drop(first);
    shared.finish_scope(first_scope);
    assert_eq!(crate::signal::lock(&shared.state).record_cache.len(), 1);

    let second_scope = shared.begin_scope().unwrap();
    let second = shared.reserve(second_scope, "second".into(), None).unwrap();
    assert_eq!(Arc::as_ptr(&second), address);
    assert!(!second.completion().done());
    assert_eq!(second.lock().name, "second");
    shared.complete(&second, None);
    drop(second);
    shared.finish_scope(second_scope);
}

#[test]
fn interleaved_scope_rollback_preserves_record_lookup_and_capacity() {
    let config = crate::Runtime::builder()
        .max_vthreads(3)
        .max_owned_scopes(2)
        .stack_cache_capacity(0)
        .build()
        .unwrap()
        .config();
    let shared = Shared::new(config);
    let left = shared.begin_owned(ScopeOptions::default(), true).unwrap();
    let right = shared.begin_owned(ScopeOptions::default(), true).unwrap();
    let first = shared.reserve(left, "first".into(), None).unwrap();
    let rolled_back = shared.reserve(right, "rollback".into(), None).unwrap();
    let failed = shared.reserve(left, "failed".into(), None).unwrap();

    shared.release_reservation(&rolled_back);
    shared.complete(&first, None);
    shared.complete(&failed, Some(crate::TaskFailure::SupervisorStopped));

    assert_eq!(shared.unobserved(left).failure_count(), 1);
    assert_eq!(shared.snapshot().tasks.len(), 2);
    assert_eq!(crate::signal::lock(&shared.state).record_count, 2);

    shared.finish_scope(right);
    shared.finish_scope(left);
    assert_eq!(crate::signal::lock(&shared.state).record_count, 0);
}
