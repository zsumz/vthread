#[test]
fn managed_role_is_confined_to_its_thread() {
    assert!(!super::is_managed());
    std::thread::spawn(|| {
        super::enter();
        assert!(super::is_managed());
    })
    .join()
    .unwrap();
    assert!(!super::is_managed());
}

#[cfg(feature = "runtime-evidence")]
#[test]
fn carrier_identity_is_confined_to_its_thread() {
    core::assert_eq!(super::current_carrier(), None);
    std::thread::spawn(|| {
        super::set_carrier(crate::CarrierId(3));
        core::assert_eq!(super::current_carrier(), Some(crate::CarrierId(3)));
    })
    .join()
    .unwrap();
    core::assert_eq!(super::current_carrier(), None);
}

#[test]
fn nested_payload_panics_fail_the_owner_on_task_native_and_task_local_paths() {
    struct Secondary;
    impl Drop for Secondary {
        fn drop(&mut self) {
            panic!("quarantined secondary dropped");
        }
    }
    struct Primary;
    impl Drop for Primary {
        fn drop(&mut self) {
            std::panic::panic_any(Secondary);
        }
    }
    struct LocalValue;
    impl Drop for LocalValue {
        fn drop(&mut self) {
            std::panic::panic_any(Primary);
        }
    }
    static VALUE: crate::TaskLocal<LocalValue> = crate::TaskLocal::new(|| LocalValue);
    for path in 0..3 {
        let runtime = crate::Runtime::new().unwrap();
        runtime
            .run_scope(|scope| {
                let mut child = scope.spawn("hostile payload", move || match path {
                    0 => std::panic::panic_any(Primary),
                    1 => {
                        let _ = crate::blocking::run(|| std::panic::panic_any(Primary));
                    }
                    _ => VALUE.with(|_| ()).unwrap(),
                })?;
                let _ = child.join();
                Ok(())
            })
            .unwrap();
        assert!(!runtime.snapshot().accepting);
        assert_eq!(runtime.snapshot().active, 0);
        let Err(crate::Error::ShutdownFailed(report)) = runtime.shutdown() else {
            panic!("hostile payload did not fail its runtime on path {path}");
        };
        assert!(
            report
                .failures
                .entries()
                .iter()
                .any(|failure| failure.panic().cleanup_panicked())
        );
    }
}
