use super::ApplicationRunFailure;
use crate::{Error, error::ScopeRunError};

#[test]
fn failure_accessors_and_parts_retain_all_sources() {
    let failure = ApplicationRunFailure::new(
        Some(ScopeRunError::BodyAndRuntime {
            body: 42,
            runtime: Error::Cancelled,
        }),
        Some(Error::BlockingFailed),
    );
    assert_eq!(failure.body(), Some(&42));
    assert!(matches!(failure.scope(), Some(Error::Cancelled)));
    assert!(matches!(failure.shutdown(), Some(Error::BlockingFailed)));
    let (body, scope, shutdown) = failure.into_parts();
    assert_eq!(body, Some(42));
    assert!(matches!(scope, Some(Error::Cancelled)));
    assert!(matches!(shutdown, Some(Error::BlockingFailed)));
}

#[test]
fn standard_error_source_uses_body_then_scope_then_shutdown() {
    use std::error::Error as _;
    let failure = ApplicationRunFailure::new(
        Some(ScopeRunError::BodyAndRuntime {
            body: std::io::Error::other("application source"),
            runtime: Error::Cancelled,
        }),
        Some(Error::BlockingFailed),
    );
    assert!(failure.source().unwrap().is::<std::io::Error>());
    let display = failure.to_string();
    assert!(display.contains("application source"));
    assert!(display.contains("scope/runtime"));
    assert!(display.contains("shutdown"));
    let failure: ApplicationRunFailure<std::io::Error> = ApplicationRunFailure::new(
        Some(ScopeRunError::Runtime(Error::Cancelled)),
        Some(Error::BlockingFailed),
    );
    assert!(matches!(
        failure.source().unwrap().downcast_ref::<Error>(),
        Some(Error::Cancelled)
    ));
    let failure: ApplicationRunFailure<std::io::Error> =
        ApplicationRunFailure::new(None, Some(Error::BlockingFailed));
    assert!(matches!(
        failure.source().unwrap().downcast_ref::<Error>(),
        Some(Error::BlockingFailed)
    ));
}
