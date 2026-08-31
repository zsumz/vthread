use super::{CAPACITY, Permit, State};
use crate::{Error, signal::lock};
use std::sync::Arc;

#[test]
fn reservation_is_bounded_and_failed_start_releases_its_slot() {
    let state = Arc::new(State::default());
    let mut permits = Vec::new();
    for _ in 0..CAPACITY {
        permits.push(Permit::reserve(Arc::clone(&state)).unwrap());
    }
    assert!(matches!(
        Permit::reserve(Arc::clone(&state)),
        Err(Error::Capacity {
            resource: crate::error::CapacityResource::Lifecycles,
            ..
        })
    ));
    drop(permits.pop());
    let replacement = Permit::reserve(Arc::clone(&state)).unwrap();
    assert_eq!(lock(&state.slots).occupied, CAPACITY);
    drop(permits);
    drop(replacement);
    assert_eq!(lock(&state.slots).occupied, 0);
}

#[test]
fn coordinator_spawn_failure_returns_before_any_runtime_work_is_constructed() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let shared = Arc::new(crate::control::Shared::new(crate::RuntimeConfig::default()));
    shared.fail_coordinator_start.store(true, Ordering::Relaxed);
    let observed = Arc::downgrade(&shared);
    let ran = Arc::new(AtomicBool::new(false));
    let body_ran = Arc::clone(&ran);
    let result = super::start(Arc::clone(&shared), move || {
        body_ran.store(true, Ordering::Relaxed);
    });
    assert!(matches!(
        result,
        Err(Error::ThreadStart {
            component: crate::ThreadComponent::Coordinator,
            ..
        })
    ));
    assert!(!ran.load(Ordering::Relaxed));
    assert!(shared.services.get().is_none());
    assert_eq!(shared.snapshot().active(), 0);
    drop(shared);
    assert!(
        observed.upgrade().is_none(),
        "failed start retained a runtime owner"
    );
}
