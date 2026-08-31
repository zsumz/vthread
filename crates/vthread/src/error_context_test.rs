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

#[test]
fn recovering_the_original_io_error_transfers_custom_source_ownership() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    #[derive(Debug)]
    struct Cause(Arc<AtomicUsize>);
    impl std::fmt::Display for Cause {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("cause")
        }
    }
    impl std::error::Error for Cause {}
    impl Drop for Cause {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let io =
        IoFailure::new("read", "path", io::Error::other(Cause(Arc::clone(&drops)))).into_io_error();
    assert!(io.get_ref().unwrap().is::<Cause>());
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    drop(io);
    assert_eq!(drops.load(Ordering::Relaxed), 1);
}
