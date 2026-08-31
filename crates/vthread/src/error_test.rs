use std::{error::Error as _, io};

use crate::{Error, PanicReport, TaskId};

#[test]
fn panic_errors_include_identity_and_name() {
    let error = Error::task_panicked(
        TaskId::new(7),
        "worker".to_owned(),
        PanicReport::capture(Box::new("boom")),
    );
    assert_eq!(error.to_string(), "task 7 (worker) panicked: boom");
}

#[test]
fn stack_errors_preserve_the_os_error() {
    let error = Error::StackAllocation(io::Error::other("no memory"));
    assert!(error.to_string().contains("no memory"));
    assert!(error.source().is_some());
}
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
