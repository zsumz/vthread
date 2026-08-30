use super::{FiberState, Suspension};

#[test]
fn public_state_types_are_comparable() {
    assert_eq!(
        FiberState::Suspended(Suspension::YieldNow),
        FiberState::Suspended(Suspension::YieldNow)
    );
}
