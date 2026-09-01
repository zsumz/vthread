#[test]
fn discovered_work_preserves_all_results_under_a_four_task_budget() {
    let report = super::run().unwrap();
    assert_eq!(report.checksum, 120);
    assert_eq!(report.visited, 15);
    assert!(report.capacity_fallbacks > 0);
    assert_eq!(report.cancelled, 1);
    assert_eq!(report.application_failures, 1);
}
