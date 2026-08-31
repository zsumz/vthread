//! Bounded queued/running native jobs and cancellation-safe queue leases.

#[path = "worker.rs"]
mod worker;

use crate::{Error, Result, ServiceSnapshot, signal::lock, wait::WaitRegistration};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};
use vthread_stack::ParkToken;

struct Job {
    token: ParkToken,
    wake: WaitRegistration,
    body: Box<dyn FnOnce() -> bool + Send>,
    reclaim: Reclaim,
}
struct Reclaim {
    abandoned: Arc<AtomicBool>,
    body: Box<dyn FnOnce() + Send>,
}
struct State {
    queue: VecDeque<Job>,
    completed: VecDeque<Reclaim>,
    running: usize,
    discarding: usize,
    panicked: u64,
    stopped: bool,
    failed: bool,
}
struct Inner {
    state: Mutex<State>,
    changed: Condvar,
    capacity: usize,
    #[cfg(test)]
    fail_worker: AtomicBool,
}
pub(crate) struct Pool {
    inner: Arc<Inner>,
    workers: Arc<crate::join_slot::JoinSlots>,
    owner: std::sync::Weak<crate::control::Shared>,
}
pub(crate) struct Lease {
    inner: Arc<Inner>,
    abandoned: Arc<AtomicBool>,
}

impl Pool {
    pub(crate) fn new(
        threads: usize,
        capacity: usize,
        owner: std::sync::Weak<crate::control::Shared>,
    ) -> Result<Self> {
        let pool = Self {
            inner: Arc::new(Inner {
                state: Mutex::new(State {
                    queue: VecDeque::new(),
                    completed: VecDeque::new(),
                    running: 0,
                    discarding: 0,
                    panicked: 0,
                    stopped: false,
                    failed: false,
                }),
                changed: Condvar::new(),
                capacity,
                #[cfg(test)]
                fail_worker: AtomicBool::new(false),
            }),
            workers: Arc::default(),
            owner,
        };
        if let Some(owner) = pool.owner.upgrade() {
            let _ = owner.resources.native.set(Arc::clone(&pool.workers));
        }
        for index in 0..threads {
            let inner = Arc::clone(&pool.inner);
            let owner = pool.owner.clone();
            let worker = thread::Builder::new()
                .name(format!("vthread-blocking-{index}"))
                .spawn(move || {
                    crate::worker_context::attach(
                        owner.clone(),
                        crate::ThreadComponent::NativeWorker,
                    );
                    worker::run(inner, owner);
                })
                .map_err(|error| {
                    Error::thread_start(crate::ThreadComponent::NativeWorker, error)
                })?;
            pool.workers.push(worker);
        }
        Ok(pool)
    }

    pub(crate) fn submit(
        &self,
        abandoned: Arc<AtomicBool>,
        token: ParkToken,
        wake: WaitRegistration,
        body: Box<dyn FnOnce() -> bool + Send>,
        reclaim: Box<dyn FnOnce() + Send>,
    ) -> Result<Lease> {
        let mut state = lock(&self.inner.state);
        if state.failed {
            return Err(Error::BlockingFailed);
        }
        if state.stopped {
            return Err(Error::RuntimeStopped);
        }
        if state.queue.len() + state.completed.len() + state.running + state.discarding
            >= self.inner.capacity
        {
            return Err(Error::Capacity {
                resource: crate::error::CapacityResource::NativeJobs,
                limit: self.inner.capacity,
            });
        }
        state.queue.push_back(Job {
            token,
            wake,
            body,
            reclaim: Reclaim {
                abandoned: Arc::clone(&abandoned),
                body: reclaim,
            },
        });
        self.inner.changed.notify_one();
        Ok(Lease {
            inner: Arc::clone(&self.inner),
            abandoned,
        })
    }

    pub(crate) fn stop(&self) {
        let mut state = lock(&self.inner.state);
        state.stopped = true;
        self.inner.changed.notify_all();
    }
    pub(crate) fn join(&self) {
        self.workers
            .join_all(&self.owner, crate::ThreadComponent::NativeWorker);
    }
    pub(crate) fn cleanup_complete(&self) -> bool {
        let state = lock(&self.inner.state);
        self.workers.joined()
            && state.queue.is_empty()
            && state.completed.is_empty()
            && state.running == 0
            && state.discarding == 0
    }
    pub(crate) fn is_failed(&self) -> bool {
        lock(&self.inner.state).failed
    }
    pub(crate) fn is_stopped(&self) -> bool {
        lock(&self.inner.state).stopped
    }
    pub(crate) fn snapshot(&self, snapshot: &mut ServiceSnapshot) {
        let state = lock(&self.inner.state);
        snapshot.blocking_queued = state.queue.len();
        snapshot.blocking_running = state.running;
        snapshot.blocking_completed = state.completed.len();
        snapshot.blocking_discarding = state.discarding;
        snapshot.blocking_capacity = self.inner.capacity;
        snapshot.blocking_panics = state.panicked;
        snapshot.blocking_failed = state.failed;
    }
}
impl Drop for Pool {
    fn drop(&mut self) {
        self.stop();
        self.join();
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.abandoned.store(true, Ordering::Release);
        // Ownership stays native through capture/result destruction. Taking the
        // mutex closes the condition-variable check/wait race without user cleanup.
        let _state = lock(&self.inner.state);
        self.inner.changed.notify_all();
    }
}

#[cfg(test)]
#[path = "pool_test.rs"]
mod pool_test;

#[cfg(test)]
#[path = "shutdown_test.rs"]
mod shutdown_test;

#[cfg(test)]
#[path = "ownership_test.rs"]
mod ownership_test;
