//! Shared admission and diagnostics; no stacks or coroutines cross this boundary.

#[path = "control_admission.rs"]
mod control_admission;
#[path = "control_scope.rs"]
mod control_scope;
#[path = "control_wait.rs"]
mod control_wait;

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock},
};

use crate::{
    CarrierId, CarrierSnapshot, CarrierStatus, RuntimeConfig, RuntimeSnapshot, TaskFailure, TaskId,
    TaskStatus,
    inbox::Inbox,
    signal::{Signal, lock},
    task::{SharedTaskRecord, TaskRecord},
};

pub(crate) struct Shared {
    pub(crate) services: OnceLock<crate::services::Services>,
    pub(crate) config: RuntimeConfig,
    pub(crate) inboxes: Vec<Arc<Inbox>>,
    pub(crate) changed: Signal,
    state: Mutex<State>,
    #[cfg(test)]
    pub(crate) fail_after_resume: std::sync::atomic::AtomicBool,
}

struct State {
    accepting: bool,
    active_scope: Option<u64>,
    scopes: BTreeMap<u64, control_scope::ScopeState>,
    next_scope: u64,
    next_task: u64,
    cursor: usize,
    active: usize,
    loads: Vec<usize>,
    rejected: u64,
    spawned: u64,
    activity: u64,
    records: BTreeMap<TaskId, SharedTaskRecord>,
    carriers: Vec<CarrierSnapshot>,
}

impl Shared {
    pub(crate) fn new(config: RuntimeConfig) -> Self {
        Self {
            services: OnceLock::new(),
            config,
            changed: Signal::default(),
            #[cfg(test)]
            fail_after_resume: std::sync::atomic::AtomicBool::new(false),
            inboxes: (0..config.carriers())
                .map(|_| {
                    Arc::new(Inbox::new(
                        config.carrier_queue_capacity(),
                        config.max_vthreads(),
                    ))
                })
                .collect(),
            state: Mutex::new(State {
                accepting: true,
                active_scope: None,
                scopes: BTreeMap::new(),
                next_scope: 1,
                next_task: 1,
                cursor: 0,
                active: 0,
                loads: vec![0; config.carriers()],
                rejected: 0,
                spawned: 0,
                activity: 0,
                records: BTreeMap::new(),
                carriers: (0..config.carriers())
                    .map(|id| CarrierSnapshot::new(CarrierId(id)))
                    .collect(),
            }),
        }
    }

    pub(crate) fn request_stop(&self) {
        let tokens = {
            let mut state = lock(&self.state);
            state.accepting = false;
            state
                .scopes
                .values()
                .map(|scope| scope.options.cancellation.clone())
                .collect::<Vec<_>>()
        };
        for inbox in &self.inboxes {
            inbox.stop();
        }
        if let Some(services) = self.services.get() {
            services.stop();
        }
        for token in tokens {
            token.cancel();
        }
        self.changed.notify();
    }

    pub(crate) fn publish(&self, snapshot: CarrierSnapshot) {
        let index = snapshot.id.0;
        let mut state = lock(&self.state);
        state.carriers[index] = snapshot;
        if state.carriers.iter().all(|carrier| {
            matches!(
                carrier.status,
                CarrierStatus::Stopped | CarrierStatus::Failed
            )
        }) {
            state.accepting = false;
        }
        drop(state);
        self.changed.notify();
    }

    pub(crate) fn transition(
        &self,
        record: &SharedTaskRecord,
        update: impl FnOnce(&mut TaskRecord),
    ) {
        let mut state = lock(&self.state);
        update(&mut lock(record));
        state.activity = state.activity.wrapping_add(1);
        drop(state);
        self.changed.notify();
    }

    pub(crate) fn complete(&self, record: &SharedTaskRecord, failure: Option<TaskFailure>) {
        let mut state = lock(&self.state);
        let mut record = lock(record);
        if record.status.is_terminal() {
            return;
        }
        record.failure = failure;
        record.deadline = None;
        record.status = if failure.is_some() {
            TaskStatus::Aborted
        } else if record.panic.is_some() {
            TaskStatus::Panicked
        } else {
            TaskStatus::Completed
        };
        if let Some(scope) = state.scopes.get_mut(&record.scope) {
            match record.status {
                TaskStatus::Aborted => scope.aborted += 1,
                TaskStatus::Panicked => scope.panicked += 1,
                _ => scope.completed += 1,
            }
        }
        let completion = Arc::clone(&record.completion);
        state.active -= 1;
        state.loads[record.carrier.0] -= 1;
        state.activity = state.activity.wrapping_add(1);
        drop(record);
        drop(state);
        completion.complete();
        self.changed.notify();
    }

    pub(crate) fn snapshot(&self) -> RuntimeSnapshot {
        let state = lock(&self.state);
        let mut snapshot = RuntimeSnapshot {
            active: state.active,
            services: self
                .services
                .get()
                .map(|services| services.snapshot())
                .unwrap_or_default(),
            ..RuntimeSnapshot::default()
        };
        for (index, carrier) in state.carriers.iter().enumerate() {
            let mut carrier = carrier.clone();
            carrier.active = state.loads[index];
            carrier.pending_starts = self.inboxes[index].pending();
            carrier.pending_wakes = self.inboxes[index].hub.pending();
            snapshot.runnable += carrier.runnable + carrier.pending_starts;
            snapshot.parked += carrier.parked;
            snapshot.timers += carrier.timers;
            snapshot.stats.add(carrier.stats);
            snapshot.stacks.add(carrier.stacks);
            snapshot.carriers.push(carrier);
        }
        snapshot.stats.spawned = state.spawned;
        snapshot.stats.rejected = state.rejected;
        snapshot.tasks = state
            .records
            .values()
            .map(|record| lock(record).snapshot())
            .collect();
        snapshot
    }
}

#[cfg(test)]
#[path = "control_test.rs"]
mod control_test;
