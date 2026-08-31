use crate::{Error, Runtime};

#[test]
fn default_policy_preserves_delayed_external_wakes() {
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    let runtime = Runtime::new().unwrap();
    let (parker, wake) = crate::park_pair();
    let shared = Arc::clone(&runtime.shared);
    let waker = std::thread::spawn(move || {
        crate::support_test::until(|| shared.snapshot().parked == 1);
        let timer = crate::signal::Signal::default();
        timer.wait(
            timer.version(),
            Some(Instant::now() + Duration::from_millis(1150)),
        );
        wake.unpark();
    });
    let result = runtime.run_scope(|scope| {
        scope
            .spawn("external wake", move || parker.park())?
            .join()?
    });
    waker.join().unwrap();
    assert!(
        matches!(result, Ok(crate::ParkOutcome::Ready)),
        "valid external wait was aborted: {result:?}"
    );
    assert!(runtime.snapshot().last_stall.is_none());
}

#[test]
fn defaults_are_bounded_and_practical() {
    let runtime = Runtime::new().expect("build runtime");
    let config = runtime.config();
    assert_eq!(config.max_vthreads(), 65_536);
    assert_eq!(config.stack_size(), 1024 * 1024);
    assert_eq!(config.stack_cache_capacity(), 64);
    assert_eq!(config.stall_policy(), crate::StallPolicy::Disabled);
}

#[test]
fn zero_capacity_is_rejected() {
    let error = Runtime::builder()
        .max_vthreads(0)
        .build()
        .expect_err("zero capacity must fail");
    assert!(matches!(
        error,
        Error::InvalidConfiguration {
            field: crate::error::ConfigurationField::MaxVthreads,
            ..
        }
    ));
}

#[test]
fn tiny_stacks_are_rejected_before_allocation() {
    let error = Runtime::builder()
        .stack_size(1024)
        .build()
        .expect_err("tiny stack must fail");
    assert!(matches!(
        error,
        Error::InvalidConfiguration {
            field: crate::error::ConfigurationField::StackSize,
            ..
        }
    ));
}

#[test]
fn carrier_limits_and_stall_deadlines_are_validated_before_starting_threads() {
    assert!(Runtime::builder().carriers(0).build().is_err());
    assert!(
        Runtime::builder()
            .carriers(2)
            .max_vthreads(1)
            .build()
            .is_err()
    );
    assert!(
        Runtime::builder()
            .carrier_queue_capacity(0)
            .build()
            .is_err()
    );
    assert!(
        Runtime::builder()
            .stall_policy(crate::StallPolicy::AbortAfter(std::time::Duration::MAX))
            .build()
            .is_err()
    );
}
