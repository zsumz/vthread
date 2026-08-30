//! Bounded queued/running native jobs and cancellation-safe queue leases.

use crate::{Error, Result, ServiceSnapshot, signal::lock, wait::WaitRegistration};
use std::{
    collections::VecDeque,
    panic::{AssertUnwindSafe, catch_unwind},
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
}
struct Inner {
    state: Mutex<State>,
    changed: Condvar,
    capacity: usize,
}
pub(crate) struct Pool {
    inner: Arc<Inner>,
    workers: Mutex<Vec<thread::JoinHandle<()>>>,
    worker_ids: Vec<thread::ThreadId>,
}
pub(crate) struct Lease {
    inner: Arc<Inner>,
    id: u64,
    abandoned: Arc<AtomicBool>,
}

impl Pool {
    pub(crate) fn new(threads: usize, capacity: usize) -> Result<Self> {
        let mut pool = Self {
            inner: Arc::new(Inner {
                state: Mutex::new(State {
                    queue: VecDeque::new(),
                    running: 0,
                    discarding: 0,
                    panicked: 0,
                    next: 1,
                    stopped: false,
                }),
                changed: Condvar::new(),
                capacity,
            }),
            workers: Mutex::new(Vec::new()),
            worker_ids: Vec::new(),
        };
        for index in 0..threads {
            let inner = Arc::clone(&pool.inner);
            let worker = thread::Builder::new()
                .name(format!("vthread-blocking-{index}"))
                .spawn(move || work(inner))?;
            pool.worker_ids.push(worker.thread().id());
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
        for worker in lock(&self.workers).drain(..) {
            if worker.thread().id() != thread::current().id() {
                let _ = worker.join();
            }
        }
    }
    pub(crate) fn is_stopped(&self) -> bool {
        lock(&self.inner.state).stopped
    }
    pub(crate) fn owns_current_thread(&self) -> bool {
        self.worker_ids.contains(&thread::current().id())
    }
    pub(crate) fn snapshot(&self, snapshot: &mut ServiceSnapshot) {
        let state = lock(&self.inner.state);
        snapshot.blocking_queued = state.queue.len();
        snapshot.blocking_running = state.running;
        snapshot.blocking_discarding = state.discarding;
        snapshot.blocking_capacity = self.inner.capacity;
        snapshot.blocking_panics = state.panicked;
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

fn work(inner: Arc<Inner>) {
    loop {
        let (job, discard) = {
            let mut state = lock(&inner.state);
            while state.queue.is_empty() && !state.stopped {
                state = inner
                    .changed
                    .wait(state)
                    .unwrap_or_else(|poison| poison.into_inner());
            }
            let Some(job) = state.queue.pop_front() else {
                return;
            };
            let discard = state.stopped;
            if discard {
                state.discarding += 1;
            } else {
                state.running += 1;
            }
            (job, discard)
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            if discard {
                job.wake.select_closed(job.token);
                drop(job.body);
                false
            } else {
                (job.body)()
            }
        }));
        if result.is_err() {
            job.wake.select_closed(job.token);
        }
        let panicked = !matches!(result, Ok(false));
        // Retain the capacity charge through panic-payload cleanup as well.
        let cleanup_panicked = catch_unwind(AssertUnwindSafe(|| drop(result))).is_err();
        let mut state = lock(&inner.state);
        if discard {
            state.discarding -= 1;
        } else {
            state.running -= 1;
        }
        state.panicked += u64::from(panicked || cleanup_panicked);
    }
}

#[cfg(test)]
#[path = "pool_test.rs"]
mod pool_test;

#[cfg(test)]
#[path = "shutdown_test.rs"]
mod shutdown_test;
