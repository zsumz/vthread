#[test]
fn panic_payload_destruction_is_inside_the_capture_boundary() {
    struct Payload;
    impl Drop for Payload {
        fn drop(&mut self) {
            panic!("payload destructor");
        }
    }
    let captured = std::panic::catch_unwind(|| super::PanicReport::capture(Box::new(Payload)));
    assert!(captured.is_ok(), "payload cleanup escaped panic capture");
}
