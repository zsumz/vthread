use super::*;
#[test]
fn completed_shutdown_remains_idempotent() {
    let runtime = crate::Runtime::new().unwrap();
    let first: ShutdownReport = runtime.shutdown().unwrap();
    let second = runtime.shutdown_until(std::time::Instant::now()).unwrap();
    assert!(matches!(second, ShutdownOutcome::Complete(report) if report == first));
}
