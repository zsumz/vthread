use crate::{Error, error::ScopeRunError};

#[test]
fn local_domain_error_preserves_borrow_and_reclaims_children() {
    crate::run(|scope| {
        scope
            .spawn("local domain", || {
                let value = String::from("domain error");
                let mut reclaimed = false;
                struct Mark<'a>(&'a mut bool);
                impl Drop for Mark<'_> {
                    fn drop(&mut self) {
                        *self.0 = true;
                    }
                }
                let result = crate::try_local_scope(|local| {
                    let mark = Mark(&mut reclaimed);
                    let _child = local
                        .spawn("borrowed", move || {
                            let _mark = mark;
                            crate::sleep(std::time::Duration::from_secs(60))
                        })
                        .unwrap();
                    Err::<(), _>(&value)
                });
                assert!(matches!(result, Err(ScopeRunError::Body(error)) if error == &value));
                assert!(reclaimed);
            })?
            .join()
    })
    .unwrap();
}

#[test]
fn local_generic_entry_outside_a_task_reports_runtime_failure() {
    assert!(matches!(
        crate::try_local_scope(|_| Ok::<_, &str>(())),
        Err(ScopeRunError::Runtime(Error::OutsideVThread))
    ));
}

#[test]
fn local_domain_and_child_failures_survive_without_cancelling_the_parent() {
    crate::run(|scope| {
        scope
            .spawn("local failures", || {
                let result = crate::try_local_scope(|local| {
                    let child = local
                        .spawn("unobserved", || panic!("child failure"))
                        .unwrap();
                    while !child.is_finished() {
                        crate::yield_now().unwrap();
                    }
                    Err::<(), _>("domain failure")
                });
                let Err(ScopeRunError::BodyAndRuntime { body, runtime }) = result else {
                    panic!("local body and child failures must both survive");
                };
                assert_eq!(body, "domain failure");
                assert!(matches!(runtime, Error::TaskPanicked { .. }));
                crate::checkpoint().unwrap();
            })?
            .join()
    })
    .unwrap();
}

#[test]
fn local_generic_body_failure_preserves_a_local_deadline() {
    use std::time::{Duration, Instant};
    crate::run(|scope| {
        scope
            .spawn("local deadline", || {
                let result = crate::try_local_scope_with_deadline(
                    Instant::now() + Duration::from_millis(10),
                    |_| {
                        crate::sleep(Duration::from_millis(30)).unwrap();
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
                crate::checkpoint().unwrap();
            })?
            .join()
    })
    .unwrap();
}
