//! Atomic bounded admission of transferable and carrier-local work.

use super::{Shared, control_scope::ScopeRecord};
use crate::{
    CarrierId, Error, Result, TaskId, TaskStatus,
    id_map::IdHashSet,
    inbox::SpawnPacket,
    join::JoinOutcome,
    options::{SpawnOptions, SpawnParent, TaskOptions},
    signal::lock,
    task::{SharedTaskRecord, TaskCell, TaskRecord},
};
use std::sync::Arc;

pub(crate) struct Spawned<T> {
    pub(crate) id: TaskId,
    pub(crate) cell: Arc<dyn JoinOutcome<T>>,
    pub(crate) record: SharedTaskRecord,
}

struct Reservation {
    record: SharedTaskRecord,
    id: TaskId,
    owner: usize,
}

impl Shared {
    pub(crate) fn reserve(
        &self,
        scope: u64,
        name: String,
        local: Option<(CarrierId, TaskId, TaskOptions)>,
    ) -> Result<SharedTaskRecord> {
        self.reserve_with(scope, name, local, SpawnOptions::default(), None)
            .map(|reservation| reservation.record)
    }

    fn reserve_with(
        &self,
        scope: u64,
        name: String,
        local: Option<(CarrierId, TaskId, TaskOptions)>,
        options: SpawnOptions,
        parent: Option<SpawnParent>,
    ) -> Result<Reservation> {
        if name.trim().is_empty() {
            return Err(Error::invalid_configuration(
                crate::error::ConfigurationField::TaskName,
                "must not be empty",
            ));
        }
        if name.len() > 128 {
            return Err(Error::LimitExceeded {
                resource: crate::error::LimitResource::TaskNameBytes,
                limit: 128,
            });
        }
        let mut state = lock(&self.state);
        let scope_state = state.scopes.get(&scope).ok_or(Error::ScopeClosed)?;
        if local.is_none() && !scope_state.admitting {
            return Err(Error::ScopeClosed);
        }
        if !state.accepting || scope_state.aborting.is_some() {
            return Err(Error::RuntimeStopped);
        }
        let options = local.as_ref().map_or_else(
            || scope_state.options.spawned(scope, parent.as_ref(), options),
            |local| local.2.clone(),
        );
        options.check()?;
        if state.record_count >= self.config.max_vthreads() {
            let mut reclaimed = IdHashSet::default();
            for scope in state.scopes.values_mut() {
                scope.records.retain(|entry| {
                    let record = entry.record.lock();
                    let remove = record.status.is_terminal() && record.outcome_observed;
                    if remove {
                        reclaimed.insert(entry.id);
                    }
                    !remove
                });
                scope
                    .progress
                    .retain_failed_tasks(|task| !reclaimed.contains(&task));
            }
            state.record_count -= reclaimed.len();
        }
        if state.record_count >= self.config.max_vthreads() {
            state.rejected += 1;
            #[cfg(feature = "runtime-evidence")]
            self.record_admission_rejected(
                crate::error::CapacityResource::Tasks,
                self.config.max_vthreads(),
            );
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
            let cursor = state.cursor;
            state.loads.select(
                cursor,
                |index| self.inboxes[index].retired_tasks(),
                |index| self.inboxes[index].can_accept(),
            )
        };
        let Some(owner) = owner else {
            state.rejected += 1;
            #[cfg(feature = "runtime-evidence")]
            self.record_admission_rejected(
                crate::error::CapacityResource::CarrierQueue,
                self.config.carrier_queue_capacity(),
            );
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
        let task = TaskRecord {
            id,
            scope,
            parent: local
                .map(|local| local.1)
                .or_else(|| parent.map(|parent| parent.id)),
            options: Some(options),
            name,
            carrier: CarrierId(owner),
            deadline: None,
            failure: None,
            status: TaskStatus::Queued,
            parks: 0,
            last_suspension: None,
            last_wake: None,
            outcome_observed: false,
            panic: None,
        };
        let record = if let Some(mut record) = state.record_cache.pop() {
            Arc::get_mut(&mut record)
                .expect("cached task cell must be unique")
                .reuse(task);
            record
        } else {
            Arc::new(TaskCell::new(task, self.config.max_vthreads()))
        };
        state.record_count += 1;
        state.loads.increment(owner);
        state.admitted += 1;
        if let Some(scope) = state.scopes.get_mut(&scope) {
            scope.records.push(ScopeRecord {
                id,
                record: Arc::clone(&record),
            });
            scope.admitted = scope
                .admitted
                .checked_add(1)
                .expect("scope admission count overflow");
            scope.progress.publish_admitted(
                scope.admitted,
                self.config.stall_policy().timeout().is_some(),
            );
        }
        state.cursor = (owner + 1) % self.inboxes.len();
        drop(state);
        if self.config.stall_policy().timeout().is_some() {
            self.changed.notify();
        }
        Ok(Reservation { record, id, owner })
    }

    pub(crate) fn release_reservation(&self, record: &SharedTaskRecord) {
        let mut state = lock(&self.state);
        let record = record.lock();
        state.record_count -= 1;
        state.loads.decrement(record.carrier.0);
        state.admitted -= 1;
        state.rejected += 1;
        if let Some(scope) = state.scopes.get_mut(&record.scope) {
            let index = scope
                .records
                .binary_search_by_key(&record.id, |entry| entry.id)
                .expect("reserved task belongs to its scope");
            scope.records.remove(index);
            scope.admitted -= 1;
            scope.progress.publish_admitted(
                scope.admitted,
                self.config.stall_policy().timeout().is_some(),
            );
        }
        drop(record);
        drop(state);
        self.changed.notify();
    }

    #[cfg(test)]
    pub(crate) fn submit<T: Send + 'static, F: FnOnce() -> T + Send + 'static>(
        &self,
        scope: u64,
        name: String,
        entry: F,
    ) -> Result<Spawned<T>> {
        self.submit_with(scope, SpawnOptions::default(), name, entry, None)
    }

    pub(crate) fn submit_with<T: Send + 'static, F: FnOnce() -> T + Send + 'static>(
        &self,
        scope: u64,
        options: SpawnOptions,
        name: String,
        entry: F,
        parent: Option<SpawnParent>,
    ) -> Result<Spawned<T>> {
        #[cfg(feature = "lifecycle-profiling")]
        let reservation_started = std::time::Instant::now();
        let Reservation { record, id, owner } =
            self.reserve_with(scope, name, None, options, parent)?;
        #[cfg(feature = "lifecycle-profiling")]
        let reservation_elapsed = reservation_started.elapsed();
        #[cfg(feature = "lifecycle-profiling")]
        let envelope_started = std::time::Instant::now();
        let (entry, cell) = crate::task_body::transferable(entry);
        let packet = SpawnPacket {
            record: Arc::clone(&record),
            entry: Some(entry),
        };
        #[cfg(feature = "lifecycle-profiling")]
        let envelope_elapsed = envelope_started.elapsed();
        #[cfg(feature = "lifecycle-profiling")]
        let inbox_started = std::time::Instant::now();
        if let Err(packet) = self.inboxes[owner].push(packet) {
            self.release_reservation(&record);
            drop(packet);
            let error = if self.inboxes[owner].stopped() {
                Error::RuntimeStopped
            } else {
                #[cfg(feature = "runtime-evidence")]
                self.record_admission_rejected(
                    crate::error::CapacityResource::CarrierQueue,
                    self.config.carrier_queue_capacity(),
                );
                Error::Capacity {
                    resource: crate::error::CapacityResource::CarrierQueue,
                    limit: self.config.carrier_queue_capacity(),
                }
            };
            return Err(error);
        }
        #[cfg(feature = "lifecycle-profiling")]
        self.lifecycle_probe.record_admission(
            reservation_elapsed,
            envelope_elapsed,
            inbox_started.elapsed(),
        );
        Ok(Spawned { id, cell, record })
    }
}

#[cfg(test)]
#[path = "control_admission_test.rs"]
mod control_admission_test;
