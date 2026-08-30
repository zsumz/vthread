use crate::{Error, Runtime};

#[test]
fn defaults_are_bounded_and_practical() {
    let runtime = Runtime::new().expect("build runtime");
    let config = runtime.config();
    assert_eq!(config.max_vthreads(), 65_536);
    assert_eq!(config.stack_size(), 1024 * 1024);
    assert_eq!(config.stack_cache_capacity(), 64);
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
            field: "max_vthreads",
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
            field: "stack_size",
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
            .stall_timeout(Some(std::time::Duration::MAX))
            .build()
            .is_err()
    );
}
