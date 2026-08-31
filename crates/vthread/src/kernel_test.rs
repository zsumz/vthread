use crate::{Error, Runtime};

#[test]
fn admission_is_bounded_and_recovers_after_completion() {
    let runtime = Runtime::builder()
        .max_vthreads(1)
        .stack_cache_capacity(1)
        .build()
        .expect("build runtime");

    runtime
        .run_scope(|scope| {
            let mut first = scope.spawn("first", || 1)?;
            let error = scope
                .spawn("second", || 2)
                .expect_err("second task exceeds capacity");
            assert!(matches!(
                error,
                Error::Capacity {
                    resource: crate::error::CapacityResource::Tasks,
                    limit: 1
                }
            ));
            assert_eq!(first.join()?, 1);
            let mut second = scope.spawn("second", || 2)?;
            assert_eq!(second.join()?, 2);
            Ok(())
        })
        .expect("scope succeeds");

    assert_eq!(runtime.snapshot().stats.rejected, 1);
}

#[test]
fn empty_names_are_rejected() {
    let runtime = Runtime::new().expect("build runtime");
    let result = runtime.run_scope(|scope| {
        let error = scope
            .spawn("   ", || ())
            .expect_err("blank task name must fail");
        assert!(matches!(
            error,
            Error::InvalidConfiguration {
                field: crate::error::ConfigurationField::TaskName,
                ..
            }
        ));
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn already_admitted_tasks_yield_to_the_tail_of_the_owner_queue() {
    use crate::{CarrierId, RuntimeConfig, control::Shared, kernel::Kernel};
    use std::sync::{Arc, Mutex};
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().expect("scope");
    let trace = Arc::new(Mutex::new(Vec::new()));
    for label in ["left", "right"] {
        let trace = Arc::clone(&trace);
        shared
            .submit(scope, label.into(), move || {
                trace.lock().expect("trace").push(label);
                crate::yield_now().expect("yield");
                trace.lock().expect("trace").push(label);
            })
            .expect("submit");
    }
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();
    while kernel.tick().expect("tick") {}
    assert_eq!(
        &*trace.lock().expect("trace"),
        &["left", "right", "left", "right"]
    );
    assert_eq!(shared.snapshot().active, 0);
}
