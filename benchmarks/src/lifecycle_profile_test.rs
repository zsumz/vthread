use super::Sample;

#[test]
fn sample_requires_complete_phase_coverage() {
    let runtime = vthread::Runtime::new().unwrap();
    let before = runtime.lifecycle_profile();
    runtime
        .run_scope(|scope| {
            drop(scope.spawn("profiled", || ())?);
            Ok(())
        })
        .unwrap();
    let profile = runtime.lifecycle_profile().checked_delta(before).unwrap();

    assert!(Sample::new(profile, u128::MAX, 0, 1).is_ok());
    assert!(Sample::new(profile, u128::MAX, 0, 2).is_err());
}
