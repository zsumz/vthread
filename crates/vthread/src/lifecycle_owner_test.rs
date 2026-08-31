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
        Err(Error::LifecycleCapacity { .. })
    ));
    drop(permits.pop());
    let replacement = Permit::reserve(Arc::clone(&state)).unwrap();
    assert_eq!(lock(&state.slots).occupied, CAPACITY);
    drop(permits);
    drop(replacement);
    assert_eq!(lock(&state.slots).occupied, 0);
}
