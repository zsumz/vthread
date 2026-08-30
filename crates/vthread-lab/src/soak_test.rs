#[test]
fn mixed_traffic_finishes_and_reclaims_all_services() {
    for carriers in [1, 4] {
        let report = super::run(std::time::Duration::from_millis(20), carriers, 4).unwrap();
        assert!(report.iterations > 0);
        assert!(report.stats.completed >= 8);
        assert!(report.stats.parks > 0);
    }
}
