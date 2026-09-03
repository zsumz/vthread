//! Bounded-lag least-loaded placement from admission and retirement totals.

pub(super) struct CarrierLoads {
    assigned: Vec<u64>,
    observed_retired: Vec<u64>,
    probe: usize,
}

impl CarrierLoads {
    pub(super) fn new(carriers: usize) -> Self {
        assert!(carriers != 0, "placement requires a carrier");
        Self {
            assigned: vec![0; carriers],
            observed_retired: vec![0; carriers],
            probe: 0,
        }
    }

    pub(super) fn select(
        &mut self,
        cursor: usize,
        mut retired: impl FnMut(usize) -> u64,
        mut can_accept: impl FnMut(usize) -> bool,
    ) -> Option<usize> {
        let probe = self.probe;
        let observed = retired(probe);
        assert!(
            observed >= self.observed_retired[probe],
            "carrier retirement count moved backward"
        );
        self.observed_retired[probe] = observed;
        self.probe = (probe + 1) % self.assigned.len();

        let candidate = (0..self.assigned.len())
            .map(|offset| (cursor + offset) % self.assigned.len())
            .min_by_key(|carrier| self.cached_load(*carrier))
            .expect("placement requires a carrier");
        if can_accept(candidate) {
            return Some(candidate);
        }

        let mut selected = None;
        let mut minimum = u64::MAX;
        for offset in 0..self.assigned.len() {
            let carrier = (cursor + offset) % self.assigned.len();
            if carrier == candidate || !can_accept(carrier) {
                continue;
            }
            let load = self.cached_load(carrier);
            if load < minimum {
                minimum = load;
                selected = Some(carrier);
            }
        }
        selected
    }

    pub(super) fn increment(&mut self, carrier: usize) {
        self.assigned[carrier] = self.assigned[carrier]
            .checked_add(1)
            .expect("carrier assignment count overflow");
    }

    pub(super) fn decrement(&mut self, carrier: usize) {
        self.assigned[carrier] = self.assigned[carrier]
            .checked_sub(1)
            .expect("carrier assignment count underflow");
    }

    pub(super) fn active(&self, carrier: usize, retired: u64) -> usize {
        usize::try_from(self.load(carrier, retired)).expect("carrier load fits usize")
    }

    fn cached_load(&self, carrier: usize) -> u64 {
        self.load(carrier, self.observed_retired[carrier])
    }

    fn load(&self, carrier: usize, retired: u64) -> u64 {
        self.assigned[carrier]
            .checked_sub(retired)
            .expect("carrier retired more tasks than it was assigned")
    }
}

#[cfg(test)]
#[path = "control_placement_test.rs"]
mod control_placement_test;
