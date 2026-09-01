#[test]
fn application_and_shutdown_errors_remain_independently_inspectable() {
    let failure = super::finish::<()>(
        Err(vthread::Error::Closed),
        Err(vthread::Error::RuntimeStopped),
    )
    .unwrap_err();
    assert!(matches!(
        failure.body.as_deref(),
        Some(vthread::Error::Closed)
    ));
    assert!(matches!(
        failure.shutdown.as_deref(),
        Some(vthread::Error::RuntimeStopped)
    ));
}
