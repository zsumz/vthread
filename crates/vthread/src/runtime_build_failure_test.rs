use super::*;

#[test]
fn original_os_source_and_cleanup_cause_survive_inspection_and_extraction() {
    let failure = RuntimeBuildFailure::new(
        Error::thread_start(
            crate::ThreadComponent::Readiness,
            std::io::Error::from_raw_os_error(24),
        ),
        Error::RuntimeStopped,
    );
    assert!(
        matches!(failure.construction(), Error::ThreadStart { source, .. } if source.raw_os_error() == Some(24))
    );
    assert!(matches!(failure.shutdown(), Error::RuntimeStopped));
    assert!(std::error::Error::source(&failure).is_some());
    assert!(failure.to_string().contains("shutdown also failed"));
    let (construction, shutdown) = failure.into_parts();
    assert!(
        matches!(construction, Error::ThreadStart { source, .. } if source.raw_os_error() == Some(24))
    );
    assert!(matches!(shutdown, Error::RuntimeStopped));
}
