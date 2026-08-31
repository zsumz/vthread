use super::{MESSAGE_LIMIT, capture};

#[test]
fn utf8_text_is_bounded_and_truncation_is_explicit() {
    let report = capture(Box::new("🙂".repeat(MESSAGE_LIMIT)));
    assert!(report.truncated);
    assert_eq!(report.message.len(), MESSAGE_LIMIT);
    assert!(!report.cleanup_panicked);
}

#[test]
fn opaque_secondary_payload_is_not_dropped() {
    struct Secondary;
    impl Drop for Secondary {
        fn drop(&mut self) {
            panic!("secondary must remain quarantined");
        }
    }
    struct Primary;
    impl Drop for Primary {
        fn drop(&mut self) {
            std::panic::panic_any(Secondary);
        }
    }
    let report = capture(Box::new(Primary));
    assert_eq!(report.message, "non-string panic payload");
    assert!(report.cleanup_panicked);
}

#[test]
fn opaque_quarantine_has_a_process_limit() {
    const CHILD: &str = "VTHREAD_QUARANTINE_LIMIT_CHILD";
    if std::env::var_os(CHILD).is_none() {
        use std::os::unix::process::ExitStatusExt;
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "panic_payload::panic_payload_test::opaque_quarantine_has_a_process_limit",
            ])
            .env(CHILD, "1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert_eq!(
            status.signal(),
            Some(6),
            "exhausted quarantine must stop the process"
        );
        return;
    }
    std::panic::set_hook(Box::new(|_| {}));
    struct Payload;
    impl Drop for Payload {
        fn drop(&mut self) {
            std::panic::panic_any(Payload);
        }
    }
    for _ in 0..=super::QUARANTINE_LIMIT {
        capture(Box::new(Payload));
    }
    panic!("quarantine exceeded its fixed bound");
}
