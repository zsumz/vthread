//! Independently published carrier snapshots outside admission state.

use crate::{CarrierId, CarrierSnapshot, CarrierStatus, signal::lock};
use std::sync::Mutex;

#[repr(align(64))]
struct CarrierSlot(Mutex<CarrierSnapshot>);

pub(super) struct CarrierStates {
    slots: Vec<CarrierSlot>,
}

impl CarrierStates {
    pub(super) fn new(carriers: usize) -> Self {
        Self {
            slots: (0..carriers)
                .map(|index| CarrierSlot(Mutex::new(CarrierSnapshot::new(CarrierId(index)))))
                .collect(),
        }
    }

    pub(super) fn publish(&self, snapshot: CarrierSnapshot) {
        let index = snapshot.id.0;
        *lock(&self.slots[index].0) = snapshot;
    }

    pub(super) fn snapshot(&self) -> Vec<CarrierSnapshot> {
        self.slots
            .iter()
            .map(|slot| lock(&slot.0).clone())
            .collect()
    }

    pub(super) fn all_terminal(&self) -> bool {
        self.slots.iter().all(|slot| {
            matches!(
                lock(&slot.0).status,
                CarrierStatus::Stopped | CarrierStatus::Failed
            )
        })
    }

    pub(super) fn failed(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| lock(&slot.0).status == CarrierStatus::Failed)
            .count()
    }
}

#[cfg(test)]
#[path = "control_carriers_test.rs"]
mod control_carriers_test;
