use super::RunFailure;
use crate::Error;

#[test]
fn dual_failure_preserves_owned_causes() {
    let failure = RunFailure::new(Error::Cancelled, Error::BlockingFailed);
    assert!(matches!(failure.scope(), Error::Cancelled));
    assert!(matches!(failure.shutdown(), Error::BlockingFailed));
    let (scope, shutdown) = failure.into_parts();
    assert!(matches!(scope, Error::Cancelled));
    assert!(matches!(shutdown, Error::BlockingFailed));
}
