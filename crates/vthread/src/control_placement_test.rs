use super::CarrierLoads;
use std::cell::Cell;

#[test]
fn balanced_cursor_uses_one_capacity_probe() {
    let mut loads = CarrierLoads::new(4);
    let probes = Cell::new(0);

    assert_eq!(
        loads.select(
            0,
            |_| 0,
            |_| {
                probes.set(probes.get() + 1);
                true
            },
        ),
        Some(0)
    );
    assert_eq!(probes.get(), 1);
}

#[test]
fn retirement_is_observed_within_one_carrier_cycle() {
    let mut loads = CarrierLoads::new(3);
    loads.increment(0);
    loads.increment(1);
    loads.increment(2);

    let retired = |carrier| u64::from(carrier == 1);
    assert_eq!(loads.select(0, retired, |_| true), Some(0));
    assert_eq!(loads.select(0, retired, |_| true), Some(1));
}

#[test]
fn unavailable_minimum_does_not_hide_available_capacity() {
    let mut loads = CarrierLoads::new(3);
    assert_eq!(loads.select(0, |_| 0, |carrier| carrier != 0), Some(1));
}
