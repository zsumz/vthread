use crate::{CarrierId, Error, Runtime, StallPolicy, control::Shared};
use std::time::Duration;

#[test]
fn placement_rotates_between_equal_loads_and_respects_queue_capacity() {
    let config = Runtime::builder()
        .carriers(2)
        .carrier_queue_capacity(1)
        .build()
        .expect("config")
        .config();
    let shared = Shared::new(config);
    let scope = shared.begin_scope().expect("scope");
    shared.submit(scope, "left".into(), || ()).expect("left");
    shared.submit(scope, "right".into(), || ()).expect("right");
    let snapshot = shared.snapshot();
    assert_eq!(snapshot.tasks[0].carrier, CarrierId(0));
    assert_eq!(snapshot.tasks[1].carrier, CarrierId(1));
    assert!(matches!(
        shared.submit(scope, "full".into(), || ()),
        Err(Error::Capacity {
            resource: crate::error::CapacityResource::CarrierQueue,
            ..
        })
    ));
    assert_eq!(shared.snapshot().active, 2);
}

#[test]
fn placement_observes_retirement_within_one_carrier_cycle() {
    let config = Runtime::builder()
        .carriers(3)
        .carrier_queue_capacity(4)
        .build()
        .expect("config")
        .config();
    let shared = Shared::new(config);
    let scope = shared.begin_scope().expect("scope");
    let first = shared.reserve(scope, "first".into(), None).unwrap();
    let second = shared.reserve(scope, "second".into(), None).unwrap();
    let third = shared.reserve(scope, "third".into(), None).unwrap();

    assert_eq!(first.lock().carrier, CarrierId(0));
    assert_eq!(second.lock().carrier, CarrierId(1));
    assert_eq!(third.lock().carrier, CarrierId(2));

    shared.complete(&second, None);
    let bounded_lag = shared.reserve(scope, "bounded lag".into(), None).unwrap();
    let replacement = shared
        .reserve(scope, "retired carrier".into(), None)
        .unwrap();
    assert_eq!(replacement.lock().carrier, CarrierId(1));

    shared.complete(&first, None);
    shared.complete(&third, None);
    shared.complete(&bounded_lag, None);
    shared.complete(&replacement, None);
    shared.finish_scope(scope);
}

#[test]
fn completed_unobserved_records_are_bounded_and_join_restores_capacity() {
    let runtime = Runtime::builder()
        .max_vthreads(1)
        .stack_cache_capacity(1)
        .build()
        .expect("runtime");
    runtime
        .run_scope(|scope| {
            let mut first = scope.spawn("first", || 1)?;
            crate::support_test::until(|| first.is_finished());
            assert!(matches!(
                scope.spawn("unobserved", || 2),
                Err(Error::Capacity {
                    resource: crate::error::CapacityResource::Tasks,
                    limit: 1
                })
            ));
            assert_eq!(first.join()?, 1);
            assert_eq!(scope.spawn("next", || 2)?.join()?, 2);
            assert_eq!(scope.runtime_snapshot().tasks.len(), 1);
            Ok(())
        })
        .expect("scope");
}

#[test]
fn names_are_bounded_and_whitespace_is_preserved() {
    let runtime = crate::Runtime::new().unwrap();
    runtime
        .run_scope(|scope| {
            assert!(matches!(
                scope.spawn("é".repeat(65), || ()),
                Err(Error::LimitExceeded { limit: 128, .. })
            ));
            let mut task = scope.spawn("  intentional name  ", || ())?;
            task.wait()?;
            assert_eq!(
                scope.runtime_snapshot().tasks()[0].name(),
                "  intentional name  "
            );
            task.join()?;
            Ok(())
        })
        .unwrap();
}

#[test]
fn admission_without_stall_detection_does_not_publish_a_control_change() {
    let shared = Shared::new(Runtime::builder().build().unwrap().config());
    let scope = shared.begin_scope().unwrap();
    let observed = shared.changed.version();

    let record = shared
        .reserve(scope, "quiet admission".into(), None)
        .unwrap();
    assert_eq!(shared.changed.version(), observed);

    shared.complete(&record, None);
    shared.finish_scope(scope);
}

#[test]
fn admission_notifies_enabled_stall_detection() {
    let config = Runtime::builder()
        .stall_policy(StallPolicy::ReportAfter(Duration::from_secs(1)))
        .build()
        .unwrap()
        .config();
    let shared = Shared::new(config);
    let scope = shared.begin_scope().unwrap();
    let observed = shared.changed.version();

    let record = shared
        .reserve(scope, "observed admission".into(), None)
        .unwrap();
    assert!(shared.changed.version() > observed);

    shared.complete(&record, None);
    shared.finish_scope(scope);
}
