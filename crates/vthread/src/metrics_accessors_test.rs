#[test]
fn admission_counts_do_not_depend_on_join_observation() {
    let runtime = crate::Runtime::new().unwrap();
    runtime
        .run_scope(|scope| {
            drop(scope.spawn("unobserved", || ())?);
            Ok(())
        })
        .unwrap();
    assert_eq!(runtime.snapshot().stats().admitted(), 1);
    assert_eq!(runtime.snapshot().stats().completed(), 1);
}
