//! A bounded process service retains and joins every runtime coordinator.

use crate::{
    Error, Result, ShutdownPhase,
    control::Shared,
    signal::{Signal, lock},
};
use std::{
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

const CAPACITY: usize = 256;
static OWNER: OnceLock<Mutex<Option<Owner>>> = OnceLock::new();

struct Owner {
    state: Arc<State>,
    // Process-lifetime ownership is deliberate: no per-runtime JoinHandle is detached.
    _worker: thread::JoinHandle<()>,
}

#[derive(Default)]
struct State {
    slots: Mutex<Slots>,
    changed: Signal,
}

#[derive(Default)]
struct Slots {
    occupied: usize,
    entries: Vec<Entry>,
}

struct Entry {
    worker: thread::JoinHandle<()>,
    shared: Arc<Shared>,
}

struct Permit {
    state: Arc<State>,
    installed: bool,
}

impl Permit {
    fn reserve(state: Arc<State>) -> Result<Self> {
        let mut slots = lock(&state.slots);
        if slots.occupied == CAPACITY {
            return Err(Error::LifecycleCapacity { limit: CAPACITY });
        }
        slots.occupied += 1;
        drop(slots);
        Ok(Self {
            state,
            installed: false,
        })
    }

    fn install(mut self, entry: Entry) {
        lock(&self.state.slots).entries.push(entry);
        self.installed = true;
        self.state.changed.notify();
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        if !self.installed {
            lock(&self.state.slots).occupied -= 1;
        }
    }
}

pub(crate) fn start(shared: Arc<Shared>, body: impl FnOnce() + Send + 'static) -> Result<()> {
    let mut owner = lock(OWNER.get_or_init(|| Mutex::new(None)));
    if owner.is_none() {
        let state = Arc::new(State::default());
        let service = Arc::clone(&state);
        let worker = thread::Builder::new()
            .name("vthread-lifecycle-owner".into())
            .stack_size(256 * 1024)
            .spawn(move || reap(service))?;
        *owner = Some(Owner {
            state,
            _worker: worker,
        });
    }
    let permit = Permit::reserve(Arc::clone(
        &owner.as_ref().expect("initialized owner").state,
    ))?;
    drop(owner);
    let runtime = Arc::downgrade(&shared);
    let worker = thread::Builder::new()
        .name("vthread-shutdown".into())
        .stack_size(256 * 1024)
        .spawn(move || {
            crate::worker_context::attach(runtime, crate::ThreadComponent::Coordinator);
            body();
        })?;
    permit.install(Entry { worker, shared });
    Ok(())
}

fn reap(state: Arc<State>) {
    crate::worker_context::enter();
    loop {
        let observed = state.changed.version();
        let (ready, pending) = {
            let mut slots = lock(&state.slots);
            let mut ready = Vec::new();
            let mut index = 0;
            while index < slots.entries.len() {
                if slots.entries[index].worker.is_finished() {
                    ready.push(slots.entries.swap_remove(index));
                } else {
                    index += 1;
                }
            }
            (ready, !slots.entries.is_empty())
        };
        for entry in ready {
            // is_finished is only a hint. join also waits for OS TLS destructors.
            crate::thread_failure::join(
                entry.worker,
                &Arc::downgrade(&entry.shared),
                crate::ThreadComponent::Coordinator,
            );
            let phase = if lock(&entry.shared.failures).is_empty() {
                ShutdownPhase::Complete
            } else {
                ShutdownPhase::Failed
            };
            entry.shared.advance_shutdown(phase);
            lock(&state.slots).occupied -= 1;
        }
        let deadline = pending.then(|| Instant::now() + Duration::from_millis(10));
        state.changed.wait(observed, deadline);
    }
}

#[cfg(test)]
#[path = "lifecycle_owner_test.rs"]
mod lifecycle_owner_test;
