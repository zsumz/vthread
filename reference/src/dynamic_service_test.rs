#[test]
fn dynamic_service_reaps_handlers_and_drains_individual_policy_outcomes() {
    let report = super::run(3).unwrap();
    assert_eq!(report.accepted, 12);
    assert_eq!(
        (
            report.echoed,
            report.blocking,
            report.cancelled,
            report.expired
        ),
        (3, 3, 3, 3)
    );
    assert!(report.used_another_carrier);
}
