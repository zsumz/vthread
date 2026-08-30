use crate::{Error, Runtime};

#[test]
fn admission_is_bounded_and_recovers_after_completion() {
    let runtime = Runtime::builder()
        .max_vthreads(1)
        .stack_cache_capacity(1)
        .build()
        .expect("build runtime");

    runtime
        .scope(|scope| {
            let first = scope.spawn("first", || 1)?;
            let error = scope
                .spawn("second", || 2)
                .expect_err("second task exceeds capacity");
            assert!(matches!(error, Error::AtCapacity { limit: 1 }));
            assert_eq!(first.join()?, 1);
            let second = scope.spawn("second", || 2)?;
            assert_eq!(second.join()?, 2);
            Ok(())
        })
        .expect("scope succeeds");

    assert_eq!(runtime.snapshot().stats.rejected, 1);
}

#[test]
fn empty_names_are_rejected() {
    let runtime = Runtime::new().expect("build runtime");
    let result = runtime.scope(|scope| {
        let error = scope
            .spawn("   ", || ())
            .expect_err("blank task name must fail");
        assert!(matches!(
            error,
            Error::InvalidConfiguration {
                field: "task name",
                ..
            }
        ));
        Ok(())
    });
    assert!(result.is_ok());
}
