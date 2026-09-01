#[test]
fn public_tcp_protocol_exercises_payload_and_terminal_connection_paths() {
    let report = crate::dynamic_service::run(1).unwrap();
    assert_eq!(
        (
            report.echoed,
            report.blocking,
            report.cancelled,
            report.expired
        ),
        (1, 1, 1, 1)
    );
}
