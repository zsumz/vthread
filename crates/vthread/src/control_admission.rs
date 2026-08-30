//! Atomic bounded admission and rotating least-load placement of unstarted work.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex},
};

use super::Shared;
use crate::{
    CarrierId, Error, PanicReport, Result, TaskId, TaskStatus,
    inbox::SpawnPacket,
    join::JoinCell,
    signal::lock,
    task::{SharedTaskRecord, TaskRecord},
};

pub(crate) struct Spawned<T> {
    pub(crate) id: TaskId,
    pub(crate) name: Arc<str>,
    pub(crate) cell: Arc<Mutex<JoinCell<T>>>,
    pub(crate) record: SharedTaskRecord,
}

impl Shared {
    pub(crate) fn submit<T: Send + 'static, F: FnOnce() -> T + Send + 'static>(
        &self,
        scope: u64,
        name: String,
        entry: F,
    ) -> Result<Spawned<T>> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::invalid_configuration(
                "task name",
                "must not be empty",
            ));
        }
        let name: Arc<str> = Arc::from(name);
        let mut state = lock(&self.state);
        if !state.accepting || state.active_scope != Some(scope) || state.aborting.is_some() {
            return Err(Error::RuntimeStopped);
        }
        // Retain useful diagnostics, but evict observed terminal records at the admission limit.
        if state.records.len() >= self.config.max_vthreads() {
            state.records.retain(|_, record| {
                let record = lock(record);
                !(record.status.is_terminal() && record.outcome_observed)
            });
        }
        if state.records.len() >= self.config.max_vthreads() {
            state.rejected += 1;
            return Err(Error::AtCapacity {
                limit: self.config.max_vthreads(),
            });
        }
        let owner = (0..self.inboxes.len())
            .map(|offset| (state.cursor + offset) % self.inboxes.len())
            .filter(|index| self.inboxes[*index].can_accept())
            .min_by_key(|index| state.loads[*index]);
        let Some(owner) = owner else {
            state.rejected += 1;
            return Err(Error::CarrierQueueFull);
        };
        let id = TaskId::new(state.next_task);
        state.next_task = state
            .next_task
            .checked_add(1)
            .ok_or(Error::Invariant("task id space exhausted"))?;
        let record = Arc::new(Mutex::new(TaskRecord {
            id,
            scope,
            name: Arc::clone(&name),
            carrier: CarrierId(owner),
            deadline: None,
            failure: None,
            status: TaskStatus::Queued,
            mounts: 0,
            yields: 0,
            parks: 0,
            last_suspension: None,
            last_wake: None,
            outcome_observed: false,
            panic: None,
        }));
        let cell = Arc::new(Mutex::new(JoinCell { outcome: None }));
        let body_cell = Arc::clone(&cell);
        let body_record = Arc::clone(&record);
        let packet = SpawnPacket {
            record: Arc::clone(&record),
            entry: Some(Box::new(move || {
                let outcome = catch_unwind(AssertUnwindSafe(entry)).map_err(PanicReport::capture);
                if let Err(panic) = &outcome {
                    lock(&body_record).panic = Some(panic.clone());
                }
                lock(&body_cell).outcome = Some(outcome);
            })),
        };
        state.records.insert(id, Arc::clone(&record));
        state.active += 1;
        state.loads[owner] += 1;
        if let Err(packet) = self.inboxes[owner].push(packet) {
            state.records.remove(&id);
            state.active -= 1;
            state.loads[owner] -= 1;
            state.rejected += 1;
            drop(state);
            drop(packet);
            return Err(Error::CarrierQueueFull);
        }
        state.spawned += 1;
        state.activity = state.activity.wrapping_add(1);
        state.cursor = (owner + 1) % self.inboxes.len();
        drop(state);
        self.changed.notify();
        Ok(Spawned {
            id,
            name,
            cell,
            record,
        })
    }
}

#[cfg(test)]
#[path = "control_admission_test.rs"]
mod control_admission_test;
