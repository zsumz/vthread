//! Bounded process ownership and health of every runtime coordinator.

#[path = "lifecycle_reaper.rs"]
mod lifecycle_reaper;

use crate::{
    Error, Result, ThreadFailure,
    control::Shared,
    signal::{Signal, lock},
};
use std::{
    sync::{Arc, Mutex, OnceLock, atomic::AtomicBool, mpsc},
    thread,
};

/// Fixed process-wide admission limit, including construction and unfinished shutdown.
/// Exhaustion returns `Error::Capacity` for `CapacityResource::Lifecycles`.
pub const LIFECYCLE_CAPACITY: usize = 256;
static OWNER: OnceLock<Mutex<Option<Owner>>> = OnceLock::new();

/// Process lifecycle service health. Failure is permanent until process exit.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LifecycleHealth {
    /// No runtime has started the process service yet.
    NotStarted,
    /// The process service can accept coordinators.
    Running,
    /// The process service failed; retained coordinators remain process-owned.
    Failed(ThreadFailure),
}

/// Observes lifecycle health without starting a service or accepting work.
pub fn lifecycle_health() -> LifecycleHealth {
    let Some(owner) = OWNER.get() else {
        return LifecycleHealth::NotStarted;
    };
    let owner = lock(owner);
    owner
        .as_ref()
        .map_or(LifecycleHealth::NotStarted, Owner::health)
}

pub(crate) fn check_health() -> Result<()> {
    match lifecycle_health() {
        LifecycleHealth::Failed(failure) => Err(Error::LifecycleFailed(Box::new(failure))),
        _ => Ok(()),
    }
}

struct Owner {
    state: Arc<State>,
    // Process-lifetime ownership is deliberate; failure retains all unjoined handles.
    worker: thread::JoinHandle<()>,
}

impl Owner {
    fn health(&self) -> LifecycleHealth {
        if self.worker.is_finished() {
            self.state.fail(lifecycle_reaper::stopped_failure());
        }
        lock(&self.state.slots)
            .failure
            .clone()
            .map_or(LifecycleHealth::Running, LifecycleHealth::Failed)
    }
}

#[derive(Default)]
struct State {
    slots: Mutex<Slots>,
    changed: Signal,
    #[cfg(test)]
    fail_at: std::sync::atomic::AtomicUsize,
}

#[derive(Default)]
struct Slots {
    occupied: usize,
    entries: Vec<Entry>,
    failure: Option<ThreadFailure>,
}

struct Entry {
    worker: Option<thread::JoinHandle<()>>,
    shared: Arc<Shared>,
    resources: Arc<CoordinatorResources>,
}

/// Retained outside the coordinator's stack, including during premature unwind.
#[derive(Default)]
pub(crate) struct CoordinatorResources {
    pub(crate) workers: Arc<Mutex<Vec<thread::JoinHandle<()>>>>,
    pub(crate) drained: AtomicBool,
}

impl State {
    fn fail(&self, failure: ThreadFailure) {
        let runtimes = {
            let mut slots = lock(&self.slots);
            if slots.failure.is_some() {
                return;
            }
            slots.failure = Some(failure.clone());
            slots
                .entries
                .iter()
                .map(|entry| Arc::clone(&entry.shared))
                .collect::<Vec<_>>()
        };
        for shared in runtimes {
            record_failure(&shared, &failure);
        }
        self.changed.notify();
    }
}

fn record_failure(shared: &Shared, failure: &ThreadFailure) {
    if shared.shutdown_phase() != crate::ShutdownPhase::Complete {
        shared.record_failure(failure.clone());
        shared.request_stop();
    }
}

struct Permit {
    state: Arc<State>,
    installed: bool,
}

impl Permit {
    fn reserve(state: Arc<State>) -> Result<Self> {
        let mut slots = lock(&state.slots);
        if let Some(failure) = &slots.failure {
            return Err(Error::LifecycleFailed(Box::new(failure.clone())));
        }
        if slots.occupied == LIFECYCLE_CAPACITY {
            return Err(Error::Capacity {
                resource: crate::error::CapacityResource::Lifecycles,
                limit: LIFECYCLE_CAPACITY,
            });
        }
        slots.occupied += 1;
        drop(slots);
        Ok(Self {
            state,
            installed: false,
        })
    }

    fn install(mut self, entry: Entry) -> Result<()> {
        let shared = Arc::clone(&entry.shared);
        let failure = {
            let mut slots = lock(&self.state.slots);
            // A failure racing spawn still retains the coordinator and its slot.
            slots.entries.push(entry);
            self.installed = true;
            slots.failure.clone()
        };
        self.state.changed.notify();
        if let Some(failure) = failure {
            record_failure(&shared, &failure);
            return Err(Error::LifecycleFailed(Box::new(failure)));
        }
        Ok(())
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        if !self.installed {
            lock(&self.state.slots).occupied -= 1;
        }
    }
}

pub(crate) fn start(
    shared: Arc<Shared>,
    resources: Arc<CoordinatorResources>,
    body: impl FnOnce() + Send + 'static,
) -> Result<()> {
    let mut owner = lock(OWNER.get_or_init(|| Mutex::new(None)));
    if owner.is_none() {
        let state = Arc::new(State::default());
        let service = Arc::clone(&state);
        let worker = thread::Builder::new()
            .name("vthread-lifecycle-owner".into())
            .stack_size(256 * 1024)
            .spawn(move || lifecycle_reaper::run(service))
            .map_err(|error| Error::thread_start(crate::ThreadComponent::LifecycleOwner, error))?;
        *owner = Some(Owner { state, worker });
    }
    let owner = owner.as_ref().expect("initialized owner");
    if let LifecycleHealth::Failed(failure) = owner.health() {
        return Err(Error::LifecycleFailed(Box::new(failure)));
    }
    let permit = Permit::reserve(Arc::clone(&owner.state))?;
    #[cfg(test)]
    if shared
        .fail_coordinator_start
        .load(std::sync::atomic::Ordering::Relaxed)
    {
        return Err(Error::thread_start(
            crate::ThreadComponent::Coordinator,
            std::io::Error::other("injected coordinator spawn failure"),
        ));
    }
    let runtime = Arc::downgrade(&shared);
    let (admitted, gate) = mpsc::sync_channel(1);
    let worker = thread::Builder::new()
        .name("vthread-shutdown".into())
        .stack_size(256 * 1024)
        .spawn(move || {
            // Failed installation closes this gate before any runtime work is built.
            if gate.recv().is_ok() {
                crate::worker_context::attach(runtime, crate::ThreadComponent::Coordinator);
                body();
            }
        })
        .map_err(|error| Error::thread_start(crate::ThreadComponent::Coordinator, error))?;
    permit.install(Entry {
        worker: Some(worker),
        shared,
        resources,
    })?;
    let _ = admitted.send(());
    Ok(())
}

#[cfg(test)]
#[path = "lifecycle_owner_test.rs"]
mod lifecycle_owner_test;

#[cfg(test)]
#[path = "lifecycle_failure_test.rs"]
mod lifecycle_failure_test;
