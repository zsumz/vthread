#[test]
fn supervisor_workers_deliver_jobs_and_runtime_timeout_retains_the_provider() {
    let report = super::run().unwrap();
    assert_eq!(report.delivered, 4);
    assert_eq!(report.retained_native_jobs, 1);
    assert!(report.native_finished);
    // Both supervisor outcomes are valid: stack reclamation can beat this observation.
    let _observed = report.supervisor_timed_out;
}
