use crate::{Error, Runtime, error::ScopeRunError};
use std::{cell::Cell, rc::Rc};

#[test]
fn generic_body_error_is_borrowed_non_send_and_caller_owned() {
    struct Domain<'a>(&'a Rc<Cell<usize>>);
    impl Drop for Domain<'_> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let count = Rc::new(Cell::new(0));
    let runtime = Runtime::new().unwrap();
    let result = runtime.try_run_scope(|_| Err::<(), _>(Domain(&count)));
    assert!(matches!(&result, Err(ScopeRunError::Body(_))));
    let _ = format!("{:?}", runtime.snapshot());
    runtime
        .run_scope(|_| Err::<(), _>(Error::WouldBlock))
        .unwrap_err();
    runtime.shutdown().unwrap();
    drop(runtime);
    assert_eq!(count.get(), 0);
    drop(result);
    assert_eq!(count.get(), 1);
}

#[test]
fn body_and_unobserved_child_failure_survive_generic_scope() {
    let runtime = Runtime::new().unwrap();
    let result = runtime.try_run_scope(|scope| {
        let task = scope
            .spawn("failed child", || panic!("child failure"))
            .unwrap();
        crate::support_test::until(|| task.is_finished());
        Err::<(), _>("domain failure")
    });
    let Err(ScopeRunError::BodyAndRuntime { body, runtime }) = result else {
        panic!("body and child failures must both survive");
    };
    assert_eq!(body, "domain failure");
    assert!(matches!(runtime.primary(), Error::TaskPanicked { .. }));
}

#[test]
fn generic_body_failure_cancels_and_reclaims_children() {
    let runtime = Runtime::new().unwrap();
    let (parker, _) = crate::park_pair();
    let result = runtime.try_run_scope(|scope| {
        let _child = scope.spawn("cancel child", move || parker.park()).unwrap();
        Err::<(), _>(17)
    });
    assert!(matches!(result, Err(ScopeRunError::Body(17))));
    assert_eq!(runtime.snapshot().active(), 0);
}

#[test]
fn generic_admission_failure_never_calls_the_body() {
    let runtime = Runtime::new().unwrap();
    runtime.shutdown().unwrap();
    let result = runtime.try_run_scope(|_| -> Result<(), &str> { panic!("body must not run") });
    assert!(matches!(
        result,
        Err(ScopeRunError::Runtime(Error::RuntimeStopped))
    ));
}

#[test]
fn generic_body_error_keeps_the_deadline_failure_after_callback_overrun() {
    use std::time::{Duration, Instant};
    let runtime = Runtime::new().unwrap();
    let result = runtime.try_run_scope_with(
        crate::ScopeOptions::default().deadline(Instant::now() + Duration::from_millis(100)),
        |_| {
            let wait = crate::signal::Signal::default();
            wait.wait(
                wait.version(),
                Some(Instant::now() + Duration::from_millis(125)),
            );
            Err::<(), _>("domain failure")
        },
    );
    assert!(matches!(
        result,
        Err(ScopeRunError::BodyAndRuntime {
            body: "domain failure",
            runtime: Error::DeadlineExceeded,
        })
    ));
    assert_eq!(runtime.snapshot().active(), 0);
    runtime.run_scope(|_| Ok(())).unwrap();
}
