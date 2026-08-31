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

#[test]
fn generic_runner_preserves_every_combination_of_body_scope_and_shutdown_failures() {
    for mask in 0..8 {
        let runtime = Runtime::new().unwrap();
        if mask & 4 != 0 {
            *lock(&runtime.shared.carrier_exit_hook) = Some(Box::new(|| panic!("shutdown error")));
        }
        let outcome = super::try_run(runtime, |scope| {
            if mask & 2 != 0 {
                let mut child = scope
                    .spawn("unobserved failed child", || panic!("scope error"))
                    .unwrap();
                child.wait().unwrap();
            }
            if mask & 1 != 0 { Err(42) } else { Ok(17) }
        });
        if mask == 0 {
            assert_eq!(outcome.unwrap(), 17);
            continue;
        }
        let failure = outcome.unwrap_err();
        assert_eq!(failure.body().copied(), (mask & 1 != 0).then_some(42));
        assert_eq!(failure.scope().is_some(), mask & 2 != 0);
        assert_eq!(failure.shutdown().is_some(), mask & 4 != 0);
        if let Some(scope) = failure.scope() {
            assert!(matches!(scope.primary(), Error::TaskPanicked { .. }));
        }
        if let Some(shutdown) = failure.shutdown() {
            assert!(matches!(shutdown, Error::ShutdownFailed(_)));
        }
    }
}

#[test]
fn generic_public_runner_keeps_borrowed_non_send_unformattable_error_with_caller() {
    use std::{cell::Cell, rc::Rc};
    struct Domain<'a>(&'a str, Rc<Cell<bool>>);
    impl Drop for Domain<'_> {
        fn drop(&mut self) {
            self.1.set(true);
        }
    }
    let message = String::from("borrowed application error");
    let dropped = Rc::new(Cell::new(false));
    let result = crate::try_run(|_| Err::<(), _>(Domain(&message, Rc::clone(&dropped))));
    let Err(failure) = result else {
        panic!("body failure lost")
    };
    assert_eq!(failure.body().unwrap().0, message);
    assert!(!dropped.get());
    assert!(failure.scope().is_none());
    assert!(failure.shutdown().is_none());
    let (body, scope, shutdown) = failure.into_parts();
    assert!(scope.is_none() && shutdown.is_none());
    assert!(!dropped.get());
    drop(body);
    assert!(dropped.get());
}

#[test]
fn generic_runner_rejects_virtual_and_native_worker_entry_without_calling_body() {
    crate::run(|scope| {
        scope
            .spawn("nested application", || {
                let result =
                    crate::try_run(|_| -> Result<(), &str> { panic!("body must not run") });
                let failure = result.unwrap_err();
                assert!(matches!(failure.scope(), Some(Error::InsideVThread)));
                assert!(failure.body().is_none() && failure.shutdown().is_none());
                crate::blocking::run(|| {
                    let failure = crate::try_run(|_| Ok::<_, &str>(())).unwrap_err();
                    assert!(matches!(failure.scope(), Some(Error::InsideManagedWorker)));
                    assert!(failure.body().is_none() && failure.shutdown().is_none());
                })
            })?
            .join()?
    })
    .unwrap();
}
