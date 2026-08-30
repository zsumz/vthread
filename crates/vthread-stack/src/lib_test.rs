use super::{FiberState, ParkRequest, ParkToken, Suspension};

#[test]
fn public_state_types_are_comparable() {
    assert_eq!(
        FiberState::Suspended(Suspension::YieldNow),
        FiberState::Suspended(Suspension::YieldNow)
    );
    assert_eq!(
        ParkRequest::new(ParkToken::new(1, 2), None),
        ParkRequest::new(ParkToken::new(1, 2), None)
    );
}
