use super::{ServiceFailure, expected_service};
use vthread::{Error, Runtime};

#[test]
fn service_primary_does_not_hide_unobserved_child_panics() {
    for category in [
        ServiceFailure::Deadline,
        ServiceFailure::Malformed,
        ServiceFailure::Disconnected,
    ] {
        for count in [1, 2] {
            let body = body_error(category);
            let runtime = Runtime::new().unwrap();
            let error = runtime
                .run_scope(|scope| {
                    let mut children = Vec::new();
                    for _ in 0..count {
                        children.push(
                            scope.spawn("failed-request", || panic!("unobserved request panic"))?,
                        );
                    }
                    until(|| children.iter().all(|child| child.is_finished()));
                    drop(children);
                    Err::<(), _>(body)
                })
                .unwrap_err();
            runtime.shutdown().unwrap();
            let failure = error.scope_failure().unwrap();
            assert!(matches!(failure.child(), Some(Error::TaskPanicked { .. })));
            assert_eq!(failure.additional_child_failures(), count - 1);
            assert_eq!(
                expected_service(&error),
                None,
                "expected body must not hide failed children"
            );
        }
    }
}

fn until(mut condition: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !condition() {
        assert!(
            std::time::Instant::now() < deadline,
            "fixture synchronization timeout"
        );
        std::thread::yield_now();
    }
}

#[test]
fn expected_service_bodies_preserve_legitimate_inherited_policy_combinations() {
    for category in [
        ServiceFailure::Deadline,
        ServiceFailure::Malformed,
        ServiceFailure::Disconnected,
    ] {
        assert_eq!(expected_service(&body_error(category)), Some(category));
        for expired in [false, true] {
            let runtime = Runtime::new().unwrap();
            let error = runtime
                .run_scope(|scope| {
                    scope
                        .spawn("combined-service", move || {
                            if expired {
                                let deadline =
                                    std::time::Instant::now() + std::time::Duration::from_secs(1);
                                vthread::local_scope_with_deadline(deadline, |_| {
                                    until(|| std::time::Instant::now() >= deadline);
                                    Err::<(), _>(body_error(category))
                                })
                            } else {
                                vthread::local_scope(|local| {
                                    local.cancel();
                                    Err::<(), _>(body_error(category))
                                })
                            }
                        })?
                        .join()
                })
                .unwrap()
                .unwrap_err();
            runtime.shutdown().unwrap();
            let failure = error
                .scope_failure()
                .expect("a real combined scope failure");
            assert!(matches!(
                failure.policy(),
                Some(Error::Cancelled | Error::DeadlineExceeded)
            ));
            assert_eq!(expected_service(&error), Some(category));
        }
    }
}

#[test]
fn service_io_classification_preserves_expected_and_unexpected_kinds() {
    use std::io::ErrorKind;
    for kind in [
        ErrorKind::UnexpectedEof,
        ErrorKind::BrokenPipe,
        ErrorKind::ConnectionReset,
        ErrorKind::ConnectionAborted,
        ErrorKind::NotConnected,
    ] {
        assert_eq!(
            expected_service(&std::io::Error::from(kind).into()),
            Some(ServiceFailure::Disconnected)
        );
    }
    for error in [
        Error::WouldBlock,
        Error::Cancelled,
        std::io::Error::from(ErrorKind::PermissionDenied).into(),
        Error::ScopeFailed(std::sync::Arc::default()),
    ] {
        assert_eq!(expected_service(&error), None);
    }
}

#[test]
fn every_secondary_failure_component_rejects_both_classifiers() {
    let stopped = Error::RuntimeStopped;
    let cancelled = Error::Cancelled;
    let unexpected = Error::WouldBlock;
    let deadline = Error::DeadlineExceeded;
    let nested = Error::ScopeFailed(std::sync::Arc::default());
    for service in [false, true] {
        for case in [
            "control",
            "body",
            "policy",
            "cleanup",
            "child",
            "extra-child",
            "extra-cleanup",
            "panic",
            "no-body",
            "no-policy",
            "nested",
            "deadline-policy",
        ] {
            let mut parts = super::ScopeParts {
                body: Some(if service { &deadline } else { &stopped }),
                policy: Some(&cancelled),
                cleanup: None,
                child: None,
                additional_child: 0,
                additional_cleanup: 0,
                body_panicked: false,
            };
            match case {
                "body" => parts.body = Some(&unexpected),
                "policy" => parts.policy = Some(&unexpected),
                "cleanup" => parts.cleanup = Some(&cancelled),
                "child" => parts.child = Some(&stopped),
                "extra-child" => parts.additional_child = 1,
                "extra-cleanup" => parts.additional_cleanup = 1,
                "panic" => parts.body_panicked = true,
                "no-body" => parts.body = None,
                "no-policy" => parts.policy = None,
                "nested" => parts.body = Some(&nested),
                "deadline-policy" => parts.policy = Some(&deadline),
                _ => {}
            }
            let policy: fn(&Error) -> bool = if service {
                |policy| matches!(policy, Error::Cancelled | Error::DeadlineExceeded)
            } else {
                |policy| matches!(policy, Error::Cancelled)
            };
            let accepted = parts.body_for(policy).is_some_and(|body| {
                if service {
                    super::service_leaf(body).is_some()
                } else {
                    super::shutdown_leaf(body)
                }
            });
            assert_eq!(
                accepted,
                case == "control" || (service && case == "deadline-policy"),
                "service={service}, case={case}"
            );
        }
    }
}

fn body_error(category: ServiceFailure) -> Error {
    match category {
        ServiceFailure::Deadline => Error::DeadlineExceeded,
        ServiceFailure::Malformed => std::io::Error::from(std::io::ErrorKind::InvalidData).into(),
        ServiceFailure::Disconnected => {
            std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into()
        }
    }
}
