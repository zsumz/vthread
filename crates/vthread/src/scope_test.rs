use std::rc::Rc;

use crate::{Error, Runtime};

#[test]
fn local_tasks_can_hold_non_send_values() {
    let runtime = Runtime::new().expect("build runtime");
    runtime
        .run_scope(|scope| {
            let mut task = scope.spawn("local", || {
                let task_value = Rc::new(41);
                crate::yield_now().expect("mounted");
                *task_value + 1
            })?;
            assert_eq!(task.join()?, 42);
            Ok(())
        })
        .expect("scope succeeds");
}

#[test]
fn unobserved_child_panic_fails_scope_exit() {
    let runtime = Runtime::new().expect("build runtime");
    let error = runtime
        .run_scope(|scope| {
            let _dropped = scope.spawn("forgotten", || panic!("unobserved"))?;
            Ok(())
        })
        .expect_err("unobserved panic must fail scope");

    assert!(matches!(
        error.primary(),
        Error::TaskPanicked { name, .. } if name == "forgotten"
    ));
}

#[test]
fn nested_scopes_are_rejected() {
    let runtime = Runtime::new().expect("build runtime");
    runtime
        .run_scope(|_| {
            let error = runtime
                .run_scope(|_| Ok(()))
                .expect_err("nested scope must fail");
            assert!(matches!(error.primary(), Error::RootScopeActive));
            Ok(())
        })
        .expect("outer scope succeeds");
}
