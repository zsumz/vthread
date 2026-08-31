use super::{FailurePhase, ThreadComponent, ThreadFailure, ThreadFailures};

#[test]
fn terminal_failure_retention_is_bounded() {
    let mut failures = ThreadFailures::default();
    for _ in 0..20 {
        failures.push(ThreadFailure::new(
            ThreadComponent::Carrier,
            "worker",
            FailurePhase::Join,
            crate::PanicReport::capture(Box::new("failed")),
        ));
    }
    assert_eq!(failures.entries().len(), 8);
    assert_eq!(failures.additional(), 12);
    assert!(failures.entries()[0].cleanup_complete());
}
