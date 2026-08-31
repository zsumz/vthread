#[test]
fn snapshot_retains_runtime_identity_after_shutdown() {
    let runtime = crate::Runtime::new().unwrap();
    let before = runtime.snapshot();
    runtime.shutdown().unwrap();
    assert_eq!(before.runtime_id(), runtime.snapshot().runtime_id());
    assert_ne!(before.runtime_id(), crate::Runtime::new().unwrap().id());
}
