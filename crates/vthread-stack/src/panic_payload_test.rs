use super::{MESSAGE_LIMIT, capture};

#[test]
fn utf8_text_is_bounded_and_truncation_is_explicit() {
    let report = capture(Box::new("🙂".repeat(MESSAGE_LIMIT)));
    assert!(report.truncated);
    assert_eq!(report.message.len(), MESSAGE_LIMIT);
    assert!(!report.cleanup_panicked);
}

#[test]
fn externally_constructed_captured_payloads_are_rebounded_on_every_boundary() {
    for boundary in 0..3 {
        let payload = Box::new(super::CapturedPanic {
            message: "🙂".repeat(MESSAGE_LIMIT),
            truncated: false,
            cleanup_panicked: true,
        });
        let report = match boundary {
            0 => capture(payload),
            1 => super::capture_without_observer(payload),
            _ => super::capture_for_join(payload).0,
        };
        assert_eq!(report.message.len(), MESSAGE_LIMIT);
        assert!(report.truncated);
        assert!(report.cleanup_panicked);
    }
    let mut message = String::with_capacity(MESSAGE_LIMIT * 10);
    message.push_str("small");
    let report = capture(Box::new(super::CapturedPanic {
        message,
        truncated: true,
        cleanup_panicked: false,
    }));
    assert_eq!(report.message, "small");
    assert!(report.message.capacity() <= MESSAGE_LIMIT);
    assert!(report.truncated);
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
fn control_join_capture_retains_opaque_values_without_running_drop() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    struct Payload(Arc<AtomicBool>);
    impl Drop for Payload {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    let dropped = Arc::new(AtomicBool::new(false));
    let (report, quarantined) = super::capture_for_join(Box::new(Payload(Arc::clone(&dropped))));
    assert!(quarantined);
    assert!(!dropped.load(Ordering::Acquire));
    assert_eq!(report.message, "non-string panic payload");
    assert!(!report.cleanup_panicked);
    let (report, quarantined) = super::capture_for_join(Box::new("known inert payload"));
    assert!(!quarantined);
    assert_eq!(report.message, "known inert payload");
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
