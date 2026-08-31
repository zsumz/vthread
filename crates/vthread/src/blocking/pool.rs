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
    id: u64,
    token: ParkToken,
    wake: WaitRegistration,
    body: Box<dyn FnOnce() -> bool + Send>,
}
struct State {
    queue: VecDeque<Job>,
    running: usize,
    discarding: usize,
    panicked: u64,
    next: u64,
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
    workers: Mutex<Vec<thread::JoinHandle<()>>>,
    owner: std::sync::Weak<crate::control::Shared>,
}
pub(crate) struct Lease {
    inner: Arc<Inner>,
    id: u64,
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
                    running: 0,
                    discarding: 0,
                    panicked: 0,
                    next: 1,
                    stopped: false,
                    failed: false,
                }),
                changed: Condvar::new(),
                capacity,
                #[cfg(test)]
                fail_worker: AtomicBool::new(false),
            }),
            workers: Mutex::new(Vec::new()),
            owner,
        };
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
                })?;
            lock(&pool.workers).push(worker);
        }
        Ok(pool)
    }

    pub(crate) fn submit(
        &self,
        abandoned: Arc<AtomicBool>,
        token: ParkToken,
        wake: WaitRegistration,
        body: Box<dyn FnOnce() -> bool + Send>,
    ) -> Result<Lease> {
        let mut state = lock(&self.inner.state);
        if state.failed {
            return Err(Error::BlockingFailed);
        }
        if state.stopped {
            return Err(Error::RuntimeStopped);
        }
        if state.queue.len() + state.running + state.discarding >= self.inner.capacity {
            return Err(Error::BlockingCapacity);
        }
        let id = state.next;
        state.next = id
            .checked_add(1)
            .ok_or(Error::Invariant("blocking identity exhausted"))?;
        state.queue.push_back(Job {
            id,
            token,
            wake,
            body,
        });
        self.inner.changed.notify_one();
        Ok(Lease {
            inner: Arc::clone(&self.inner),
            id,
            abandoned,
        })
    }

    pub(crate) fn stop(&self) {
        let mut state = lock(&self.inner.state);
        state.stopped = true;
        self.inner.changed.notify_all();
    }
    pub(crate) fn join(&self) {
        let workers = std::mem::take(&mut *lock(&self.workers));
        for worker in workers {
            crate::thread_failure::join(worker, &self.owner, crate::ThreadComponent::NativeWorker);
        }
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
        let removed = {
            let mut state = lock(&self.inner.state);
            // Once stop selects queued cleanup, workers retain ownership. A carrier
            // must not run an arbitrary queued destructor while reclaiming its lease.
            if state.stopped {
                return;
            }
            state
                .queue
                .iter()
                .position(|job| job.id == self.id)
                .and_then(|index| state.queue.remove(index))
        };
        drop(removed);
    }
}

#[cfg(test)]
#[path = "pool_test.rs"]
mod pool_test;

#[cfg(test)]
#[path = "shutdown_test.rs"]
mod shutdown_test;
