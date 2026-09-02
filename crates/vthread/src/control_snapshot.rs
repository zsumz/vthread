//! Weakly consistent observations without nested locks under runtime control state.

use super::Shared;
use crate::{RuntimeSnapshot, signal::lock};

impl Shared {
    pub(crate) fn snapshot(&self) -> RuntimeSnapshot {
        let (mut snapshot, records, stall) = {
            let state = lock(&self.state);
            let mut snapshot = RuntimeSnapshot {
                shutdown_phase: state.shutdown_phase,
                accepting: state.accepting,
                active: state.active,
                carriers: state.carriers.clone(),
                ..RuntimeSnapshot::empty(self.id)
            };
            snapshot.stats.admitted = state.admitted;
            snapshot.stats.rejected = state.rejected;
            for (carrier, load) in snapshot.carriers.iter_mut().zip(&state.loads) {
                carrier.active = *load;
            }
            let records = state.records.values().cloned().collect::<Vec<_>>();
            (snapshot, records, state.last_stall.clone())
        };
        // No service, inbox, failure-store or task-record lock is acquired while
        // holding control state. Inert stall report contents are also cloned here.
        #[cfg(test)]
        if let Some(hook) = lock(&self.snapshot_observe_hook).take() {
            hook();
        }
        snapshot.last_stall = stall.as_deref().cloned();
        snapshot.failures = lock(&self.failures).clone();
        snapshot.last_scope_failure = lock(&self.last_scope_failure).clone();
        snapshot.services = self
            .services
            .get()
            .map(|services| services.snapshot())
            .unwrap_or_default();
        for (carrier, inbox) in snapshot.carriers.iter_mut().zip(&self.inboxes) {
            carrier.pending_starts = inbox.pending();
            carrier.pending_wakes = inbox.hub.pending();
            snapshot.runnable += carrier.runnable + carrier.pending_starts;
            snapshot.parked += carrier.parked;
            snapshot.timers += carrier.timers;
            snapshot.stats.add(carrier.stats);
            snapshot.stacks.add(carrier.stacks);
        }
        snapshot.tasks = records.iter().map(|record| record.snapshot()).collect();
        snapshot
    }
}

#[cfg(test)]
#[path = "control_snapshot_test.rs"]
mod control_snapshot_test;
