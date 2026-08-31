use super::super::{Entry, Permit, State};
use crate::{ThreadComponent, control::Shared, signal::lock, support_test::until};
use std::sync::{Arc, atomic::Ordering};

#[test]
fn failed_claims_retain_the_entry_before_and_after_join() {
    for phase in [2, 3] {
        let state = Arc::new(State::default());
        let shared = Arc::new(Shared::new(crate::RuntimeConfig::default()));
        let worker = std::thread::spawn(|| {});
        until(|| worker.is_finished());
        Permit::reserve(Arc::clone(&state))
            .unwrap()
            .install(Entry {
                worker: Some(worker),
                shared: Arc::clone(&shared),
                resources: Arc::default(),
            })
            .unwrap();
        state.fail_at.store(phase, Ordering::Release);
        super::run(Arc::clone(&state));
        let mut slots = lock(&state.slots);
        assert_eq!(slots.occupied, 1);
        assert_eq!(slots.entries.len(), 1);
        assert!(slots.failure.is_some());
        assert_eq!(slots.entries[0].worker.is_some(), phase == 2);
        if let Some(worker) = slots.entries[0].worker.take() {
            worker.join().unwrap();
        }
        drop(slots);
        assert_eq!(
            lock(&shared.failures).entries()[0].component(),
            ThreadComponent::LifecycleOwner
        );
        assert!(!shared.snapshot().accepting());
    }
}

#[test]
fn failure_racing_installation_keeps_the_reserved_handle_and_rejects_new_work() {
    let state = Arc::new(State::default());
    let permit = Permit::reserve(Arc::clone(&state)).unwrap();
    state.fail(super::stopped_failure());
    let shared = Arc::new(Shared::new(crate::RuntimeConfig::default()));
    let result = permit.install(Entry {
        worker: Some(std::thread::spawn(|| {})),
        shared: Arc::clone(&shared),
        resources: Arc::default(),
    });
    assert!(matches!(result, Err(crate::Error::LifecycleFailed(_))));
    assert!(matches!(
        Permit::reserve(Arc::clone(&state)),
        Err(crate::Error::LifecycleFailed(_))
    ));
    let mut slots = lock(&state.slots);
    assert_eq!(slots.occupied, 1);
    assert_eq!(slots.entries.len(), 1);
    slots.entries[0].worker.take().unwrap().join().unwrap();
    assert_eq!(
        lock(&shared.failures).entries()[0].component(),
        ThreadComponent::LifecycleOwner
    );
}
