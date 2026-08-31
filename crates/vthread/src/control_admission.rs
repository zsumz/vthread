//! Atomic bounded admission of transferable and carrier-local work.

use super::Shared;
use crate::{
    CarrierId, Error, Result, TaskId, TaskStatus,
    completion::Completion,
    inbox::SpawnPacket,
    join::JoinCell,
    options::TaskOptions,
    signal::lock,
    task::{SharedTaskRecord, TaskRecord},
};
use std::sync::{Arc, Mutex};

pub(crate) struct Spawned<T> {
    pub(crate) id: TaskId,
    pub(crate) name: Arc<str>,
    pub(crate) cell: Arc<Mutex<JoinCell<T>>>,
    pub(crate) record: SharedTaskRecord,
}

impl Shared {
    pub(crate) fn reserve(
        &self,
        scope: u64,
        name: String,
        local: Option<(CarrierId, TaskId, TaskOptions)>,
    ) -> Result<SharedTaskRecord> {
        if name.trim().is_empty() {
            return Err(Error::invalid_configuration(
                "task name",
                "must not be empty",
            ));
        }
        if name.len() > 128 {
            return Err(Error::LimitExceeded {
                resource: "task name UTF-8 bytes",
                limit: 128,
            });
        }
        let mut state = lock(&self.state);
        let scope_state = state.scopes.get(&scope).ok_or(Error::RuntimeStopped)?;
        if !state.accepting || scope_state.aborting.is_some() {
            return Err(Error::RuntimeStopped);
        }
        let options = local
            .as_ref()
            .map_or_else(|| scope_state.options.child(None), |local| local.2.clone());
        options.check()?;
        if state.records.len() >= self.config.max_vthreads() {
            state.records.retain(|_, record| {
                let record = lock(record);
                !(record.status.is_terminal() && record.outcome_observed)
            });
        }
        if state.records.len() >= self.config.max_vthreads() {
            state.rejected += 1;
            return Err(Error::Capacity {
                resource: crate::error::CapacityResource::Tasks,
                limit: self.config.max_vthreads(),
            });
        }
        let owner = if let Some((carrier, _, _)) = &local {
            if self.inboxes[carrier.0].stopped() {
                return Err(Error::RuntimeStopped);
            }
            Some(carrier.0)
        } else {
            (0..self.inboxes.len())
                .map(|offset| (state.cursor + offset) % self.inboxes.len())
                .filter(|index| self.inboxes[*index].can_accept())
                .min_by_key(|index| state.loads[*index])
        };
        let Some(owner) = owner else {
            state.rejected += 1;
            return Err(Error::Capacity {
                resource: crate::error::CapacityResource::CarrierQueue,
                limit: self.config.carrier_queue_capacity(),
            });
        };
        let id = TaskId::new(state.next_task);
        state.next_task = state.next_task.checked_add(1).ok_or(Error::fault(
            crate::error::FaultComponent::Scheduler,
            "task id space exhausted",
        ))?;
        let record = Arc::new(Mutex::new(TaskRecord {
            id,
            scope,
            parent: local.map(|local| local.1),
            options,
            completion: Arc::new(Completion::new(self.config.max_vthreads())),
            name: Arc::from(name),
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
        state.records.insert(id, Arc::clone(&record));
        state.active += 1;
        state.loads[owner] += 1;
        state.admitted += 1;
        if let Some(scope) = state.scopes.get_mut(&scope) {
            scope.activity = scope.activity.wrapping_add(1);
        }
        state.cursor = (owner + 1) % self.inboxes.len();
        drop(state);
        self.changed.notify();
        Ok(record)
    }

    pub(crate) fn release_reservation(&self, record: &SharedTaskRecord) {
        let mut state = lock(&self.state);
        let record = lock(record);
        state.records.remove(&record.id);
        state.active -= 1;
        state.loads[record.carrier.0] -= 1;
        state.admitted -= 1;
        state.rejected += 1;
        if let Some(scope) = state.scopes.get_mut(&record.scope) {
            scope.activity = scope.activity.wrapping_add(1);
        }
        drop(record);
        drop(state);
        self.changed.notify();
    }

    pub(crate) fn submit<T: Send + 'static, F: FnOnce() -> T + Send + 'static>(
        &self,
        scope: u64,
        name: String,
        entry: F,
    ) -> Result<Spawned<T>> {
        let record = self.reserve(scope, name, None)?;
        let (id, name, owner) = {
            let record = lock(&record);
            (record.id, Arc::clone(&record.name), record.carrier.0)
        };
        let cell = Arc::new(Mutex::new(JoinCell { outcome: None }));
        let body_cell = Arc::clone(&cell);
        let body_record = Arc::clone(&record);
        let packet = SpawnPacket {
            record: Arc::clone(&record),
            entry: Some(Box::new(move || {
                crate::task_body::run(&body_record, entry, move |outcome| {
                    lock(&body_cell).outcome = Some(outcome);
                });
            })),
        };
        if let Err(packet) = self.inboxes[owner].push(packet) {
            self.release_reservation(&record);
            drop(packet);
            return Err(if self.inboxes[owner].stopped() {
                Error::RuntimeStopped
            } else {
                Error::Capacity {
                    resource: crate::error::CapacityResource::CarrierQueue,
                    limit: self.config.carrier_queue_capacity(),
                }
            });
        }
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
