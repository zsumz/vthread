use crate::{RuntimeConfig, ScopeOptions, control::Shared};

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
    let supervisor = runtime.supervisor(ScopeOptions::default()).unwrap();
    assert!(matches!(
        runtime.supervisor(ScopeOptions::default()),
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
    let supervisor = runtime.supervisor(ScopeOptions::default()).unwrap();
    runtime
        .run_scope(|scope| {
            let shared = std::sync::Arc::clone(&runtime.shared);
            scope
                .spawn("local parent", move || {
                    let mounted = crate::context::current().unwrap();
                    let parent = mounted.execution().unwrap();
                    let owned = crate::signal::lock(&parent.record).scope;
                    crate::local_scope(|_| {
                        crate::local_scope(|local| {
                            assert_eq!(crate::signal::lock(&shared.state).scopes.len(), 2);
                            let mut child = local.spawn("local child", || 42)?;
                            assert_eq!(crate::signal::lock(&child.record).scope, owned);
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
