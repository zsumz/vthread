//! Shared admission and diagnostics; no stacks or coroutines cross this boundary.

#[path = "control_admission.rs"]
mod control_admission;
#[path = "control_scope.rs"]
mod control_scope;
#[path = "control_snapshot.rs"]
mod control_snapshot;
#[path = "control_wait.rs"]
mod control_wait;

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock},
};

use crate::{
    CarrierId, CarrierSnapshot, CarrierStatus, RuntimeConfig, TaskFailure, TaskId, TaskStatus,
    inbox::Inbox,
    signal::{Signal, lock},
    task::{SharedTaskRecord, TaskRecord},
};

pub(crate) struct Shared {
    pub(crate) resources: Arc<crate::lifecycle_resources::CoordinatorResources>,
    pub(crate) id: crate::identity::RuntimeId,
    #[cfg(test)]
    pub(crate) fail_coordinator_start: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    pub(crate) fail_coordinator_before_drain: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    pub(crate) coordinator_fault: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    pub(crate) carrier_fault: std::sync::atomic::AtomicUsize,
    pub(crate) services: OnceLock<crate::services::Services>,
    pub(crate) config: RuntimeConfig,
    pub(crate) inboxes: Vec<Arc<Inbox>>,
    pub(crate) changed: Signal,
    pub(crate) failures: Mutex<crate::ThreadFailures>,
    pub(crate) last_scope_failure:
        Mutex<Option<Arc<crate::scope_failure_report::ScopeFailureReport>>>,
    state: Mutex<State>,
    #[cfg(test)]
    pub(crate) fail_after_resume: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    pub(crate) coordinator_exit_hook: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    #[cfg(test)]
    pub(crate) carrier_exit_hook: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    #[cfg(test)]
    snapshot_observe_hook: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

struct State {
    shutdown_phase: crate::ShutdownPhase,
    last_stall: Option<Arc<crate::StallSnapshot>>,
    accepting: bool,
    active_scope: Option<u64>,
    scopes: BTreeMap<u64, control_scope::ScopeState>,
    next_scope: u64,
    next_task: u64,
    cursor: usize,
    active: usize,
    loads: Vec<usize>,
    rejected: u64,
    admitted: u64,
    records: BTreeMap<TaskId, SharedTaskRecord>,
    carriers: Vec<CarrierSnapshot>,
}

impl Shared {
    pub(crate) fn new(config: RuntimeConfig) -> Self {
        Self {
            resources: Arc::default(),
            id: crate::identity::RuntimeId::next(),
            #[cfg(test)]
            fail_coordinator_start: std::sync::atomic::AtomicBool::new(false),
            #[cfg(test)]
            fail_coordinator_before_drain: std::sync::atomic::AtomicBool::new(false),
            #[cfg(test)]
            coordinator_fault: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            carrier_fault: std::sync::atomic::AtomicUsize::new(0),
            services: OnceLock::new(),
            config,
            changed: Signal::default(),
            failures: Mutex::new(crate::ThreadFailures::default()),
            last_scope_failure: Mutex::new(None),
            #[cfg(test)]
            fail_after_resume: std::sync::atomic::AtomicBool::new(false),
            #[cfg(test)]
            coordinator_exit_hook: Mutex::new(None),
            #[cfg(test)]
            carrier_exit_hook: Mutex::new(None),
            #[cfg(test)]
            snapshot_observe_hook: Mutex::new(None),
            inboxes: (0..config.carriers())
                .map(|_| {
                    Arc::new(Inbox::new(
                        config.carrier_queue_capacity(),
                        config.max_vthreads(),
                    ))
                })
                .collect(),
            state: Mutex::new(State {
                shutdown_phase: crate::ShutdownPhase::NotRequested,
                last_stall: None,
                accepting: true,
                active_scope: None,
                scopes: BTreeMap::new(),
                next_scope: 1,
                next_task: 1,
                cursor: 0,
                active: 0,
                loads: vec![0; config.carriers()],
                rejected: 0,
                admitted: 0,
                records: BTreeMap::new(),
                carriers: (0..config.carriers())
                    .map(|id| CarrierSnapshot::new(CarrierId(id)))
                    .collect(),
            }),
        }
    }

    pub(crate) fn request_stop(&self) {
        // Take ownership of queued native cleanup before any carrier can observe stop
        // and reclaim a lease. Otherwise a carrier can steal a queued capture destructor.
        if let Some(services) = self.services.get() {
            services.blocking.stop();
        }
        let tokens = {
            let mut state = lock(&self.state);
            state.accepting = false;
            state.shutdown_phase = state.shutdown_phase.max(crate::ShutdownPhase::Requested);
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
            services.reactor.stop();
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

    pub(crate) fn shutdown_phase(&self) -> crate::ShutdownPhase {
        lock(&self.state).shutdown_phase
    }

    pub(crate) fn advance_shutdown(&self, phase: crate::ShutdownPhase) {
        let mut state = lock(&self.state);
        state.shutdown_phase = state.shutdown_phase.max(phase);
        drop(state);
        self.changed.notify();
    }

    pub(crate) fn record_failure(&self, mut failure: crate::ThreadFailure) {
        failure.shutdown_phase = lock(&self.state).shutdown_phase;
        lock(&self.failures).push(failure);
        self.changed.notify();
    }

    pub(crate) fn transition(
        &self,
        record: &SharedTaskRecord,
        update: impl FnOnce(&mut TaskRecord),
    ) {
        let mut state = lock(&self.state);
        let mut record = lock(record);
        update(&mut record);
        if let Some(scope) = state.scopes.get_mut(&record.scope) {
            scope.activity = scope.activity.wrapping_add(1);
        }
        drop(record);
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
            scope.activity = scope.activity.wrapping_add(1);
            match record.status {
                TaskStatus::Aborted => scope.aborted += 1,
                TaskStatus::Panicked => scope.panicked += 1,
                _ => scope.completed += 1,
            }
        }
        let completion = Arc::clone(&record.completion);
        state.active -= 1;
        state.loads[record.carrier.0] -= 1;
        drop(record);
        drop(state);
        completion.complete();
        self.changed.notify();
    }
}

#[cfg(test)]
#[path = "control_test.rs"]
mod control_test;
