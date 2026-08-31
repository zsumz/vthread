use crate::{Error, Runtime, ScopeOptions, SpawnOptions, park_pair, support_test::until};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

#[test]
fn either_cross_scope_owner_or_caller_cancels_a_dynamically_spawned_child() {
    for cancel_owner in [false, true] {
        let runtime = Runtime::builder().carriers(2).build().unwrap();
        let supervisor = runtime.supervisor().unwrap();
        runtime
            .run_scope(|scope| {
                let spawner = supervisor.spawner();
                let (sent, received) = mpsc::sync_channel(1);
                let (parent_wait, parent_wake) = park_pair();
                let mut parent = scope.spawn("foreign owner parent", move || {
                    let child = spawner.spawn("cross scope child", || {
                        let (wait, _wake) = park_pair();
                        wait.park()
                    })?;
                    sent.send(child).unwrap();
                    parent_wait.park()
                })?;
                let mut child = received.recv_timeout(Duration::from_secs(5)).unwrap();
                until(|| runtime.snapshot().parked() == 2);
                if cancel_owner {
                    supervisor.cancel();
                } else {
                    parent.cancel();
                }
                assert!(matches!(child.join()?, Err(Error::Cancelled)));
                assert!(!scope.cancellation_token().is_cancelled());
                if cancel_owner {
                    assert!(!parent.cancellation_token().is_cancelled());
                    parent_wake.unpark();
                    assert!(parent.join()?.is_ok());
                } else {
                    assert!(!supervisor.cancellation_token().is_cancelled());
                    assert!(matches!(parent.join()?, Err(Error::Cancelled)));
                }
                Ok(())
            })
            .unwrap();
        supervisor.shutdown().unwrap();
    }
}

#[test]
fn dynamic_deadline_is_the_minimum_of_owner_caller_and_request() {
    let runtime = Runtime::builder().carriers(2).build().unwrap();
    let now = Instant::now();
    for (owner_secs, parent_secs, requested_secs) in [(5, 7, 9), (9, 5, 7), (9, 7, 5)] {
        let owner = now + Duration::from_secs(owner_secs);
        let parent = now + Duration::from_secs(parent_secs);
        let requested = now + Duration::from_secs(requested_secs);
        let supervisor = runtime
            .supervisor_with(ScopeOptions::default().deadline(owner))
            .unwrap();
        runtime
            .run_scope(|scope| {
                let spawner = supervisor.spawner();
                let observed = scope
                    .spawn_with(
                        SpawnOptions::default().deadline(parent),
                        "parent",
                        move || {
                            spawner
                                .spawn_with(
                                    SpawnOptions::default().deadline(requested),
                                    "child",
                                    crate::deadline,
                                )?
                                .join()?
                        },
                    )?
                    .join()??;
                assert_eq!(observed, Some(owner.min(parent).min(requested)));
                Ok(())
            })
            .unwrap();
        supervisor.shutdown().unwrap();
    }
}

#[test]
fn foreign_runtime_caller_does_not_supply_parentage_or_inherited_policy() {
    let source = Runtime::new().unwrap();
    let target = Runtime::new().unwrap();
    let supervisor = target.supervisor().unwrap();
    source
        .run_scope(|scope| {
            let spawner = supervisor.spawner();
            let mut parent = scope.spawn_with(
                SpawnOptions::default().deadline(Instant::now() + Duration::from_secs(5)),
                "foreign runtime",
                move || spawner.spawn("target child", crate::deadline),
            )?;
            let mut child = parent.join()??;
            parent.cancel();
            assert_eq!(child.join()??, None);
            assert!(!child.cancellation_token().is_cancelled());
            let snapshot = target.snapshot();
            assert_eq!(snapshot.tasks()[0].parent(), None);
            Ok(())
        })
        .unwrap();
    supervisor.shutdown().unwrap();
}
