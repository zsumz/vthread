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

#[test]
fn joining_a_worker_never_executes_its_opaque_panic_destructor() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    struct Payload(Arc<AtomicBool>);
    impl Drop for Payload {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    let dropped = Arc::new(AtomicBool::new(false));
    let payload = Payload(Arc::clone(&dropped));
    let worker = std::thread::spawn(move || std::panic::panic_any(payload));
    let shared = Arc::new(crate::control::Shared::new(crate::RuntimeConfig::default()));
    super::join(worker, &Arc::downgrade(&shared), ThreadComponent::Carrier);
    assert!(
        !dropped.load(Ordering::Acquire),
        "control-thread join ran user code"
    );
    let failures = crate::signal::lock(&shared.failures);
    assert_eq!(failures.entries().len(), 1);
    assert!(!failures.entries()[0].cleanup_complete());
}
