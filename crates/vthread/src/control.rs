//! Shared admission and diagnostics; no stacks or coroutines cross this boundary.

#[path = "control_admission.rs"]
mod control_admission;
#[path = "control_completion.rs"]
mod control_completion;
pub(crate) use control_completion::CompletionUpdate;
#[cfg(feature = "runtime-evidence")]
#[path = "control_evidence.rs"]
mod control_evidence;
#[path = "control_scope.rs"]
mod control_scope;
#[path = "control_snapshot.rs"]
mod control_snapshot;
#[path = "control_transition.rs"]
mod control_transition;
#[path = "control_wait.rs"]
mod control_wait;

use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use crate::{
    CarrierId, CarrierSnapshot, CarrierStatus, RuntimeConfig, TaskId,
    id_map::IdMap,
    inbox::Inbox,
    signal::{Signal, lock},
    task::SharedTaskRecord,
    task_progress::CarrierProgress,
};

#[cfg(feature = "runtime-evidence")]
type EvidenceRecorder = crate::diagnostics::evidence::Recorder;
#[cfg(not(feature = "runtime-evidence"))]
type EvidenceRecorder = ();
#[cfg(feature = "lifecycle-profiling")]
type LifecycleRecorder = crate::lifecycle_probe::Recorder;

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
    pub(crate) cancellation: crate::CancellationToken,
    #[cfg(feature = "lifecycle-profiling")]
    pub(crate) lifecycle_probe: LifecycleRecorder,
    abort_requested: AtomicBool,
    #[cfg(feature = "runtime-evidence")]
    pub(crate) evidence: Option<crate::diagnostics::evidence::Recorder>,
    pub(crate) carrier_progress: Vec<CarrierProgress>,
    pub(crate) inboxes: Vec<Arc<Inbox>>,
    pub(crate) changed: Signal,
    target_waiters: AtomicUsize,
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
    pub(crate) scope_drain_hook: Mutex<Option<Box<dyn FnOnce() + Send>>>,
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
    records: IdMap<TaskId, SharedTaskRecord>,
    record_cache: Vec<SharedTaskRecord>,
    carriers: Vec<CarrierSnapshot>,
}

impl Shared {
    pub(crate) fn new(config: RuntimeConfig) -> Self {
        Self::construct(config, None)
    }

    #[cfg(feature = "runtime-evidence")]
    pub(crate) fn with_evidence(
        config: RuntimeConfig,
        evidence: crate::diagnostics::evidence::Recorder,
    ) -> Self {
        Self::construct(config, Some(evidence))
    }

    fn construct(config: RuntimeConfig, evidence: Option<EvidenceRecorder>) -> Self {
        #[cfg(not(feature = "runtime-evidence"))]
        let _ = evidence;
        let id = crate::identity::RuntimeId::next();
        #[cfg(feature = "runtime-evidence")]
        let inboxes = (0..config.carriers())
            .map(|index| {
                let inbox = evidence.as_ref().map_or_else(
                    || Inbox::new(config.carrier_queue_capacity(), config.max_vthreads()),
                    |recorder| {
                        Inbox::with_evidence(
                            config.carrier_queue_capacity(),
                            config.max_vthreads(),
                            crate::diagnostics::evidence::Emitter::new(
                                id,
                                CarrierId(index),
                                recorder.clone(),
                            ),
                        )
                    },
                );
                Arc::new(inbox)
            })
            .collect();
        #[cfg(not(feature = "runtime-evidence"))]
        let inboxes = (0..config.carriers())
            .map(|_| {
                Arc::new(Inbox::new(
                    config.carrier_queue_capacity(),
                    config.max_vthreads(),
                ))
            })
            .collect();
        Self {
            resources: Arc::default(),
            id,
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
            cancellation: crate::CancellationToken::root(config.max_vthreads()),
            #[cfg(feature = "lifecycle-profiling")]
            lifecycle_probe: LifecycleRecorder::new(),
            abort_requested: AtomicBool::new(false),
            #[cfg(feature = "runtime-evidence")]
            evidence,
            carrier_progress: (0..config.carriers())
                .map(|_| CarrierProgress::new())
                .collect(),
            changed: Signal::default(),
            target_waiters: AtomicUsize::new(0),
            failures: Mutex::new(crate::ThreadFailures::default()),
            last_scope_failure: Mutex::new(None),
            #[cfg(test)]
            fail_after_resume: std::sync::atomic::AtomicBool::new(false),
            #[cfg(test)]
            coordinator_exit_hook: Mutex::new(None),
            #[cfg(test)]
            carrier_exit_hook: Mutex::new(None),
            #[cfg(test)]
            scope_drain_hook: Mutex::new(None),
            #[cfg(test)]
            snapshot_observe_hook: Mutex::new(None),
            inboxes,
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
                records: IdMap::default(),
                record_cache: Vec::new(),
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
            let _advanced = state.shutdown_phase < crate::ShutdownPhase::Requested;
            state.shutdown_phase = state.shutdown_phase.max(crate::ShutdownPhase::Requested);
            self.abort_requested.store(true, Ordering::Release);
            #[cfg(feature = "runtime-evidence")]
            if _advanced {
                self.record(
                    crate::diagnostics::evidence::RuntimeEventKind::ShutdownAdvanced {
                        phase: crate::ShutdownPhase::Requested,
                    },
                );
            }
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
        let notify = snapshot.status != CarrierStatus::Running
            || self.config.stall_policy().timeout().is_some();
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
        if notify {
            self.changed.notify();
        }
    }

    pub(crate) fn shutdown_phase(&self) -> crate::ShutdownPhase {
        lock(&self.state).shutdown_phase
    }

    pub(crate) fn advance_shutdown(&self, phase: crate::ShutdownPhase) {
        let mut state = lock(&self.state);
        let _advanced = state.shutdown_phase < phase;
        state.shutdown_phase = state.shutdown_phase.max(phase);
        #[cfg(feature = "runtime-evidence")]
        if _advanced {
            self.record(crate::diagnostics::evidence::RuntimeEventKind::ShutdownAdvanced { phase });
        }
        drop(state);
        self.changed.notify();
    }

    pub(crate) fn record_failure(&self, mut failure: crate::ThreadFailure) {
        failure.shutdown_phase = lock(&self.state).shutdown_phase;
        lock(&self.failures).push(failure);
        self.changed.notify();
    }
}

#[cfg(test)]
#[path = "control_test.rs"]
mod control_test;
