#[test]
fn repeated_batches_preserve_connection_identity_and_transfer_new_payloads() {
    let runtime = vthread::Runtime::builder().carriers(4).build().unwrap();
    let mut pair = runtime
        .run_scope(|scope| super::start(scope, 0, None)?.finish())
        .unwrap();
    let server = pair.server.local_addr().unwrap();
    let client = pair.client.local_addr().unwrap();
    for iteration in 1..128 {
        pair = runtime
            .run_scope(|scope| super::start(scope, iteration, Some(pair))?.finish())
            .unwrap();
        assert_eq!(pair.server.local_addr().unwrap(), server);
        assert_eq!(pair.client.local_addr().unwrap(), client);
        assert_eq!(pair.server.peer_addr().unwrap(), client);
        assert_eq!(pair.client.peer_addr().unwrap(), server);
        assert_eq!(runtime.snapshot().services().readiness_waits(), 0);
    }
    drop(pair);
    runtime.shutdown().unwrap();
    assert_eq!(runtime.snapshot().stats().completed(), 256);
}
