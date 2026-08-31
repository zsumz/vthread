use crate::{Error, Runtime, local_scope, support_test::until};

#[test]
fn root_scope_preserves_body_error_and_unobserved_child_panic() {
    let runtime = Runtime::new().unwrap();
    let error = runtime
        .run_scope(|scope| {
            let child = scope.spawn("child failure", || panic!("unobserved child panic"))?;
            until(|| child.is_finished());
            drop(child);
            Err::<(), _>(Error::WouldBlock)
        })
        .unwrap_err();
    let diagnostic = format!("{error:?}");
    assert!(
        diagnostic.contains("WouldBlock") && diagnostic.contains("unobserved child panic"),
        "scope discarded a failure: {diagnostic}"
    );
}

#[test]
fn local_scope_preserves_body_error_and_unobserved_child_panic() {
    let runtime = Runtime::new().unwrap();
    runtime
        .run_scope(|scope| {
            scope
                .spawn("parent", || {
                    let error = local_scope(|local| {
                        let child =
                            local.spawn("child failure", || panic!("unobserved child panic"))?;
                        while !child.is_finished() {
                            crate::yield_now()?;
                        }
                        drop(child);
                        Err::<(), _>(Error::WouldBlock)
                    })
                    .unwrap_err();
                    let diagnostic = format!("{error:?}");
                    assert!(
                        diagnostic.contains("WouldBlock")
                            && diagnostic.contains("unobserved child panic"),
                        "scope discarded a failure: {diagnostic}"
                    );
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn root_deadline_is_observed_after_a_nonpreemptible_callback() {
    use std::time::{Duration, Instant};
    let runtime = Runtime::new().unwrap();
    let result = runtime.run_scope_with(
        crate::ScopeOptions::default().deadline(Instant::now() + Duration::from_millis(10)),
        |_| {
            let wait = crate::signal::Signal::default();
            wait.wait(
                wait.version(),
                Some(Instant::now() + Duration::from_millis(30)),
            );
            Ok(())
        },
    );
    assert!(
        result.is_err(),
        "callback overran its deadline without reporting it"
    );
}

#[test]
fn multiple_child_failures_are_counted_and_retained_after_records_are_removed() {
    let runtime = Runtime::new().unwrap();
    let error = runtime
        .run_scope(|scope| {
            let mut children = Vec::new();
            for _ in 0..3 {
                children.push(scope.spawn("failed child", || panic!("child panic"))?);
            }
            until(|| children.iter().all(|child| child.is_finished()));
            drop(children);
            Err::<(), _>(Error::WouldBlock)
        })
        .unwrap_err();
    let failure = error.scope_failure().unwrap();
    assert!(matches!(failure.body(), Some(Error::WouldBlock)));
    assert!(matches!(failure.child(), Some(Error::TaskPanicked { .. })));
    assert_eq!(failure.additional_child_failures(), 2);
    let snapshot = runtime.snapshot();
    assert!(snapshot.tasks.is_empty());
    assert_eq!(
        snapshot
            .last_scope_failure
            .as_ref()
            .unwrap()
            .additional_child_failures(),
        2
    );
}

#[test]
fn a_body_panic_rethrows_its_payload_and_retains_secondary_child_failure() {
    let runtime = Runtime::new().unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.run_scope::<()>(|scope| {
            let child = scope.spawn("child", || panic!("child failure"))?;
            until(|| child.is_finished());
            drop(child);
            panic!("root body panic");
        })
    }));
    let payload = result.unwrap_err();
    assert_eq!(payload.downcast_ref::<&str>(), Some(&"root body panic"));
    let snapshot = runtime.snapshot();
    let failure = snapshot.last_scope_failure.as_ref().unwrap();
    assert!(failure.body_panicked());
    assert!(matches!(failure.child(), Some(Error::TaskPanicked { .. })));
    assert_eq!(snapshot.active, 0);
}

#[test]
fn replacing_a_scope_failure_drops_user_io_causes_outside_the_metadata_lock() {
    use std::sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    };
    #[derive(Debug)]
    struct Cause(Weak<crate::control::Shared>, Arc<AtomicBool>);
    impl std::fmt::Display for Cause {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("user cause")
        }
    }
    impl std::error::Error for Cause {}
    impl Drop for Cause {
        fn drop(&mut self) {
            let shared = self.0.upgrade().unwrap();
            self.1.store(
                shared.last_scope_failure.try_lock().is_ok(),
                Ordering::Relaxed,
            );
        }
    }
    let runtime = Runtime::new().unwrap();
    let unlocked = Arc::new(AtomicBool::new(false));
    let cause = Cause(Arc::downgrade(&runtime.shared), Arc::clone(&unlocked));
    drop(runtime.run_scope::<()>(|_| Err(std::io::Error::other(cause).into())));
    drop(runtime.run_scope::<()>(|_| Err(Error::WouldBlock)));
    assert!(
        unlocked.load(Ordering::Relaxed),
        "user error was destroyed while holding metadata lock"
    );
}
