#[test]
fn channel_end_of_stream_drains_empty_and_populated_pipelines() {
    for count in [0, 1, 127] {
        assert_eq!(
            super::pipeline(count).unwrap(),
            count * count.saturating_sub(1) / 2
        );
    }
}

#[test]
fn tcp_transfers_preserve_binary_payloads() {
    let payload = (0..4096)
        .map(|index| (index % 256) as u8)
        .collect::<Vec<_>>();
    assert_eq!(super::tcp_echo(payload.clone()).unwrap(), payload);
}

#[test]
fn timeout_retains_native_ownership_until_release() {
    super::native_shutdown().unwrap();
}
