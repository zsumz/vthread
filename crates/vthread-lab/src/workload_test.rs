#[test]
fn cancellation_racing_a_permit_does_not_poison_runtime_reuse() {
    let runtime = vthread::Runtime::new().unwrap();
    for _ in 0..20 {
        super::cancel(&runtime).unwrap();
    }
    assert_eq!(runtime.snapshot().active, 0);
}
