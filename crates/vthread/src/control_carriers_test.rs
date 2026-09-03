use super::CarrierStates;
use crate::{CarrierId, CarrierSnapshot, CarrierStatus};

#[test]
fn carriers_publish_and_reach_terminal_state_independently() {
    let carriers = CarrierStates::new(2);
    let mut first = CarrierSnapshot::new(CarrierId(0));
    first.status = CarrierStatus::Stopped;
    carriers.publish(first.clone());
    assert!(!carriers.all_terminal());

    let mut second = CarrierSnapshot::new(CarrierId(1));
    second.status = CarrierStatus::Failed;
    carriers.publish(second.clone());
    assert!(carriers.all_terminal());
    assert_eq!(carriers.failed(), 1);
    assert_eq!(carriers.snapshot(), [first, second]);
}
