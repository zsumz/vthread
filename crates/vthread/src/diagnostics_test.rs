use crate::{RuntimeStats, StackSnapshot};

#[test]
fn diagnostic_counters_start_at_zero() {
    assert_eq!(RuntimeStats::default().mounts, 0);
    assert_eq!(StackSnapshot::default().cached, 0);
}

#[test]
fn repeated_stop_requests_preserve_completed_shutdown_diagnostics() {
    let runtime = crate::Runtime::new().unwrap();
    assert_eq!(
        runtime.snapshot().shutdown_phase,
        crate::ShutdownPhase::NotRequested
    );
    runtime.request_shutdown();
    // A pre-established coordinator may already have advanced before this observation.
    assert!(runtime.snapshot().shutdown_phase >= crate::ShutdownPhase::Requested);
    runtime.shutdown().unwrap();
    runtime.request_shutdown();
    let snapshot = runtime.snapshot();
    assert_eq!(snapshot.shutdown_phase, crate::ShutdownPhase::Complete);
    assert!(!snapshot.accepting);
    assert_eq!(snapshot.active, 0);
}
