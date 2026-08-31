//! Externally retained OS joins; interrupted claims restore unconsumed handles.

use crate::{ThreadComponent, control::Shared, signal::lock};
use std::sync::{Arc, Mutex, Weak};
use std::thread;

enum State {
    Retained(thread::JoinHandle<()>),
    Joining,
    Joined,
}

pub(crate) struct JoinSlot(Mutex<State>);

struct Claim {
    slot: Arc<JoinSlot>,
    worker: Option<thread::JoinHandle<()>>,
}

impl JoinSlot {
    pub(crate) fn new(worker: thread::JoinHandle<()>) -> Arc<Self> {
        Arc::new(Self(Mutex::new(State::Retained(worker))))
    }

    fn claim(self: &Arc<Self>) -> Option<Claim> {
        let mut state = lock(&self.0);
        if !matches!(*state, State::Retained(_)) {
            return None;
        }
        let State::Retained(worker) = std::mem::replace(&mut *state, State::Joining) else {
            unreachable!("retained state checked under lock")
        };
        Some(Claim {
            slot: Arc::clone(self),
            worker: Some(worker),
        })
    }

    pub(crate) fn finished(&self) -> bool {
        match &*lock(&self.0) {
            State::Retained(worker) => worker.is_finished(),
            State::Joined => true,
            State::Joining => false,
        }
    }

    pub(crate) fn joined(&self) -> bool {
        matches!(*lock(&self.0), State::Joined)
    }

    pub(crate) fn join(self: &Arc<Self>, owner: &Weak<Shared>, component: ThreadComponent) {
        let Some(claim) = self.claim() else { return };
        #[cfg(test)]
        inject(
            owner,
            match component {
                ThreadComponent::Carrier => 1,
                ThreadComponent::Readiness => 3,
                ThreadComponent::NativeWorker => 4,
                _ => 0,
            },
        );
        claim.join(owner, component);
    }
}

impl Claim {
    fn join(mut self, owner: &Weak<Shared>, component: ThreadComponent) {
        let worker = self.worker.as_ref().expect("retained claim");
        // All potentially panicking preparation precedes consuming the handle.
        assert_ne!(worker.thread().id(), thread::current().id(), "self join");
        let name = worker.thread().name().unwrap_or("unnamed").to_owned();
        // Join a distinct valid OS thread. No reporting, hooks or user code occur
        // between consuming this handle and recording its completed OS join.
        let result = self.worker.take().expect("retained claim").join();
        *lock(&self.slot.0) = State::Joined;
        // Consume or quarantine the OS panic payload before any injected reporting
        // failure can unwind. Opaque payload destructors never run on join callers.
        crate::thread_failure::report_join(result, owner, component, &name);
        #[cfg(test)]
        inject(owner, 6);
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            *lock(&self.slot.0) = State::Retained(worker);
        }
    }
}

#[derive(Default)]
pub(crate) struct JoinSlots(Mutex<Vec<Arc<JoinSlot>>>);

impl JoinSlots {
    pub(crate) fn push(&self, worker: thread::JoinHandle<()>) {
        lock(&self.0).push(JoinSlot::new(worker));
    }

    pub(crate) fn join_all(&self, owner: &Weak<Shared>, component: ThreadComponent) {
        let mut index = 0;
        loop {
            let Some(slot) = lock(&self.0).get(index).cloned() else {
                break;
            };
            slot.join(owner, component);
            index += 1;
            #[cfg(test)]
            if index == 1 {
                inject(
                    owner,
                    match component {
                        ThreadComponent::Carrier => 2,
                        ThreadComponent::NativeWorker => 5,
                        _ => 0,
                    },
                );
            }
        }
    }

    pub(crate) fn joined(&self) -> bool {
        lock(&self.0).iter().all(|slot| slot.joined())
    }

    #[cfg(test)]
    pub(crate) fn counts(&self) -> (usize, usize) {
        let slots = lock(&self.0);
        assert!(
            slots
                .iter()
                .all(|slot| !matches!(*lock(&slot.0), State::Joining)),
            "a completed coordinator left a join claimed"
        );
        (
            slots.len(),
            slots.iter().filter(|slot| slot.joined()).count(),
        )
    }
}

#[cfg(test)]
fn inject(owner: &Weak<Shared>, phase: usize) {
    if phase != 0
        && owner.upgrade().is_some_and(|owner| {
            owner
                .coordinator_fault
                .load(std::sync::atomic::Ordering::Acquire)
                == phase
        })
    {
        panic!("injected coordinator join failure at phase {phase}");
    }
}

#[cfg(test)]
#[path = "join_slot_test.rs"]
mod join_slot_test;
