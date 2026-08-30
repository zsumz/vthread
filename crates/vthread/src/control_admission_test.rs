use crate::{CarrierId, Error, Runtime, control::Shared};

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
        Err(Error::CarrierQueueFull)
    ));
    assert_eq!(shared.snapshot().active, 2);
}

#[test]
fn completed_unobserved_records_are_bounded_and_join_restores_capacity() {
    let runtime = Runtime::builder()
        .max_vthreads(1)
        .stack_cache_capacity(1)
        .build()
        .expect("runtime");
    runtime
        .scope(|scope| {
            let first = scope.spawn("first", || 1)?;
            crate::support_test::until(|| first.is_finished());
            assert!(matches!(
                scope.spawn("unobserved", || 2),
                Err(Error::AtCapacity { limit: 1 })
            ));
            assert_eq!(first.join()?, 1);
            assert_eq!(scope.spawn("next", || 2)?.join()?, 2);
            assert_eq!(scope.snapshot().tasks.len(), 1);
            Ok(())
        })
        .expect("scope");
}
