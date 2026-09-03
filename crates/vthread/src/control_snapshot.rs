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
                active: state
                    .scopes
                    .values()
                    .map(|scope| scope.progress.active())
                    .sum(),
                carriers: self.carrier_states.snapshot(),
                ..RuntimeSnapshot::empty(self.id)
            };
            snapshot.stats.admitted = state.admitted;
            snapshot.stats.rejected = state.rejected;
            for (index, carrier) in snapshot.carriers.iter_mut().enumerate() {
                carrier.active = state
                    .loads
                    .active(index, self.inboxes[index].retired_tasks());
            }
            let mut records = state
                .scopes
                .values()
                .flat_map(|scope| scope.records.iter().map(|entry| entry.record.clone()))
                .collect::<Vec<_>>();
            records.sort_unstable_by_key(|record| record.lock().id);
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
        let mounted = self
            .carrier_progress
            .iter()
            .map(crate::task_progress::CarrierProgress::mounted)
            .collect::<Vec<_>>();
        for (carrier, inbox) in snapshot.carriers.iter_mut().zip(&self.inboxes) {
            carrier.pending_starts = inbox.pending();
            carrier.pending_wakes = inbox.hub.pending();
            snapshot.runnable += carrier.runnable + carrier.pending_starts;
            snapshot.parked += carrier.parked;
            snapshot.timers += carrier.timers;
            snapshot.stats.add(carrier.stats);
            snapshot.stacks.add(carrier.stacks);
        }
        snapshot.tasks = records
            .iter()
            .map(|record| record.snapshot(&mounted))
            .collect();
        snapshot
    }
}

#[cfg(test)]
#[path = "control_snapshot_test.rs"]
mod control_snapshot_test;
