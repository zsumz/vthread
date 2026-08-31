use crate::{Error, Runtime, signal::lock};

#[test]
fn successful_scope_cannot_hide_a_failed_shutdown_join() {
    let runtime = Runtime::new().unwrap();
    *lock(&runtime.shared.carrier_exit_hook) = Some(Box::new(|| panic!("failed carrier exit")));
    assert!(matches!(
        super::run(runtime, |_| Ok(42)),
        Err(Error::ShutdownFailed(_))
    ));
}

#[test]
fn scope_and_shutdown_failures_are_both_returned() {
    let runtime = Runtime::new().unwrap();
    *lock(&runtime.shared.carrier_exit_hook) = Some(Box::new(|| panic!("failed carrier exit")));
    let Error::RunFailed(failure) =
        super::run(runtime, |_| Err::<(), _>(Error::WouldBlock)).unwrap_err()
    else {
        panic!("both failures must survive");
    };
    assert!(matches!(failure.scope().primary(), Error::WouldBlock));
    assert!(matches!(failure.shutdown(), Error::ShutdownFailed(_)));
}

#[test]
fn public_runner_reports_a_component_failure_observed_by_the_body() {
    struct BadPayload;
    impl Drop for BadPayload {
        fn drop(&mut self) {
            panic!("panic payload cleanup failed");
        }
    }
    let result = crate::run(|scope| {
        let mut task = scope.spawn("observe native failure", || {
            let result = crate::blocking::run(|| std::panic::panic_any(BadPayload));
            assert!(result.is_err());
        })?;
        // The application deliberately observes/handles its child failure.
        let _ = task.join();
        Ok(42)
    });
    assert!(
        result.is_err(),
        "successful body masked a component failure"
    );
    assert!(matches!(
        result,
        Err(Error::ShutdownFailed(_) | Error::RunFailed(_))
    ));
}

#[test]
fn scope_failure_with_successful_shutdown_is_not_wrapped_again() {
    assert!(matches!(
        crate::run(|_| Err::<(), _>(Error::WouldBlock)),
        Err(Error::WouldBlock)
    ));
}
