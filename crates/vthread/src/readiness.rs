//! Generation-tagged readiness subscriptions owned by a bounded zio driver.

mod driver;

use crate::{Error, Result, ServiceSnapshot, signal::lock, wait::WaitRegistration};
use std::{
    collections::BTreeMap,
    os::fd::{BorrowedFd, OwnedFd},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};
use vthread_stack::ParkToken;

struct Entry {
    fd: OwnedFd,
    interest: zio::Interest,
    token: ParkToken,
    wake: WaitRegistration,
}
struct State {
    entries: BTreeMap<u64, Entry>,
    next: u64,
    stopped: bool,
    error: Option<String>,
}
struct Inner {
    owner: std::sync::Weak<crate::control::Shared>,
    state: Mutex<State>,
    waker: zio::Waker,
    capacity: usize,
    registered: AtomicUsize,
    #[cfg(test)]
    fail_wait: std::sync::atomic::AtomicBool,
}
pub(crate) struct Reactor {
    inner: Arc<Inner>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}
pub(crate) struct Lease {
    inner: Arc<Inner>,
    key: u64,
}

impl Reactor {
    pub(crate) fn new(
        capacity: usize,
        owner: std::sync::Weak<crate::control::Shared>,
    ) -> Result<Self> {
        let (ready, receive) = std::sync::mpsc::sync_channel(1);
        let runtime = owner.clone();
        let worker = thread::Builder::new()
            .name("vthread-readiness".to_owned())
            .spawn(move || {
                crate::worker_context::attach(runtime.clone(), crate::ThreadComponent::Readiness);
                driver::start(capacity, ready, runtime);
            })
            .map_err(|error| Error::thread_start(crate::ThreadComponent::Readiness, error))?;
        match receive.recv() {
            Ok(Ok(inner)) => Ok(Self {
                inner,
                worker: Mutex::new(Some(worker)),
            }),
            result => {
                crate::thread_failure::join(worker, &owner, crate::ThreadComponent::Readiness);
                Err(match result {
                    Ok(Err(error)) => error,
                    _ => Error::fault(
                        crate::error::FaultComponent::Readiness,
                        "readiness initialization failed",
                    ),
                })
            }
        }
    }
    pub(crate) fn register(
        &self,
        fd: BorrowedFd<'_>,
        interest: zio::Interest,
        token: ParkToken,
        wake: WaitRegistration,
    ) -> Result<Lease> {
        let key = {
            let mut state = lock(&self.inner.state);
            if state.error.is_some() {
                return Err(Error::ReadinessFailed);
            }
            if state.stopped {
                return Err(Error::RuntimeStopped);
            }
            if state.entries.len() == self.inner.capacity {
                return Err(Error::Capacity {
                    resource: crate::error::CapacityResource::Readiness,
                    limit: self.inner.capacity,
                });
            }
            let key = state.next;
            state.next = key.checked_add(1).ok_or(Error::fault(
                crate::error::FaultComponent::Readiness,
                "readiness identity exhausted",
            ))?;
            state.entries.insert(
                key,
                Entry {
                    fd: crate::net::io::checked(
                        "duplicate readiness descriptor",
                        fd,
                        fd.try_clone_to_owned(),
                    )?,
                    interest,
                    token,
                    wake,
                },
            );
            key
        };
        let lease = Lease {
            inner: Arc::clone(&self.inner),
            key,
        };
        if self.inner.waker.wake().is_err() {
            self.inner.close(Some("readiness wake failed".to_owned()));
            return Err(Error::ReadinessFailed);
        }
        Ok(lease)
    }
    pub(crate) fn check(&self) -> Result<()> {
        let state = lock(&self.inner.state);
        if state.error.is_some() {
            Err(Error::ReadinessFailed)
        } else if state.stopped {
            Err(Error::RuntimeStopped)
        } else {
            Ok(())
        }
    }
    pub(crate) fn stop(&self) {
        self.inner.close(None);
    }
    pub(crate) fn join(&self) {
        let worker = lock(&self.worker).take();
        if let Some(worker) = worker {
            crate::thread_failure::join(
                worker,
                &self.inner.owner,
                crate::ThreadComponent::Readiness,
            );
        }
    }
    pub(crate) fn snapshot(&self, snapshot: &mut ServiceSnapshot) {
        let state = lock(&self.inner.state);
        snapshot.readiness_waits = state.entries.len();
        snapshot.readiness_capacity = self.inner.capacity;
        snapshot.readiness_registered = self.inner.registered.load(Ordering::Acquire);
        snapshot.readiness_failed = state.error.is_some();
        snapshot.readiness_error = state.error.clone();
    }
}
impl Drop for Reactor {
    fn drop(&mut self) {
        self.stop();
        self.join();
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let removed = lock(&self.inner.state).entries.remove(&self.key);
        if removed.is_some() && self.inner.waker.wake().is_err() {
            self.inner.close(Some("readiness wake failed".to_owned()));
        }
    }
}
impl Inner {
    fn close(&self, error: Option<String>) {
        let (entries, failure) = {
            let mut state = lock(&self.state);
            state.stopped = true;
            let failure = if state.error.is_none() {
                error.clone()
            } else {
                None
            };
            if state.error.is_none() {
                state.error = error;
            }
            (std::mem::take(&mut state.entries), failure)
        };
        if let Some(error) = failure
            && let Some(owner) = self.owner.upgrade()
        {
            owner.record_failure(crate::ThreadFailure::new(
                crate::ThreadComponent::Readiness,
                "vthread-readiness",
                crate::FailurePhase::Running,
                crate::PanicReport::capture(Box::new(error)),
            ));
        }
        for (_, entry) in entries {
            entry.wake.select_closed(entry.token);
        }
        // The driver also uses a bounded wait, so a broken wake cannot strand shutdown.
        let _ = self.waker.wake();
    }
}
fn io_error(error: impl std::error::Error + Send + Sync + 'static) -> Error {
    Error::io("readiness backend", "zio", std::io::Error::other(error))
}

#[cfg(test)]
#[path = "readiness_test.rs"]
mod readiness_test;
