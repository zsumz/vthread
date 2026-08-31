use super::*;
#[test]
fn io_context_is_bounded_without_losing_os_cause() {
    let error = IoFailure::new("read", "é".repeat(200), io::Error::from_raw_os_error(2));
    assert_eq!(error.context().len(), 256);
    assert!(error.context_truncated());
    assert_eq!(error.raw_os_error(), Some(2));
    assert_eq!(error.operation(), "read");
}
#[test]
fn fault_incidents_are_distinct_even_with_the_same_detail() {
    let first = RuntimeFault::new(FaultComponent::Scheduler, "test");
    let second = RuntimeFault::new(FaultComponent::Scheduler, "test");
    assert_ne!(first.incident_id(), second.incident_id());
}
