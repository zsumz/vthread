//! Runtime ownership of the readiness driver and explicit blocking workers.

use crate::{Result, RuntimeConfig, blocking::pool::Pool, readiness::Reactor};

/// Bounded I/O and delegated-work activity for operator diagnostics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServiceSnapshot {
    /// Outstanding readiness waits, including not-yet-installed registrations.
    pub(crate) readiness_waits: usize,
    /// Configured readiness wait limit.
    pub(crate) readiness_capacity: usize,
    /// Registrations currently retained by zio.
    pub(crate) readiness_registered: usize,
    /// Whether the readiness driver failed and rejected further waits.
    pub(crate) readiness_failed: bool,
    /// First terminal backend failure, when present.
    pub(crate) readiness_error: Option<String>,
    /// Native jobs waiting for a worker.
    pub(crate) blocking_queued: usize,
    /// Native jobs already executing; these cannot be forcibly cancelled.
    pub(crate) blocking_running: usize,
    /// Completed results retained natively until caller commit or abandonment.
    pub(crate) blocking_completed: usize,
    /// Queued captures or abandoned results being destroyed on native workers.
    pub(crate) blocking_discarding: usize,
    /// Maximum queued, running, completed, and discarding native jobs combined.
    pub(crate) blocking_capacity: usize,
    /// Native body or late-result/queued-capture destructor panics observed by workers.
    pub(crate) blocking_panics: u64,
    /// A native worker failed outside its ordinary job boundary; admission is closed.
    pub(crate) blocking_failed: bool,
}

pub(crate) struct Services {
    pub(crate) reactor: Reactor,
    pub(crate) blocking: Pool,
}

impl Services {
    pub(crate) fn new(
        config: RuntimeConfig,
        owner: std::sync::Weak<crate::control::Shared>,
    ) -> Result<Self> {
        Ok(Self {
            reactor: Reactor::new(config.io_capacity(), owner.clone())?,
            blocking: Pool::new(config.blocking_threads(), config.blocking_capacity(), owner)?,
        })
    }
    pub(crate) fn stop(&self) {
        self.blocking.stop();
        self.reactor.stop();
    }
    pub(crate) fn join(&self) {
        self.reactor.join();
        self.blocking.join();
    }
    pub(crate) fn snapshot(&self) -> ServiceSnapshot {
        let mut snapshot = ServiceSnapshot::default();
        self.reactor.snapshot(&mut snapshot);
        self.blocking.snapshot(&mut snapshot);
        snapshot
    }
}
impl Drop for Services {
    fn drop(&mut self) {
        self.stop();
        self.join();
    }
}

#[cfg(test)]
#[path = "services_test.rs"]
mod services_test;

impl ServiceSnapshot {
    /// Outstanding readiness waits, including not-yet-installed registrations.
    pub fn readiness_waits(&self) -> usize {
        self.readiness_waits
    }
    /// Configured readiness wait limit.
    pub fn readiness_capacity(&self) -> usize {
        self.readiness_capacity
    }
    /// Registrations currently retained by zio.
    pub fn readiness_registered(&self) -> usize {
        self.readiness_registered
    }
    /// Whether the readiness driver failed and rejected further waits.
    pub fn readiness_failed(&self) -> bool {
        self.readiness_failed
    }
    /// First terminal backend failure, when present.
    pub fn readiness_error(&self) -> Option<&str> {
        self.readiness_error.as_deref()
    }
    /// Native jobs waiting for a worker.
    pub fn blocking_queued(&self) -> usize {
        self.blocking_queued
    }
    /// Native jobs already executing; these cannot be forcibly cancelled.
    pub fn blocking_running(&self) -> usize {
        self.blocking_running
    }
    /// Completed results retained natively until caller commit or abandonment.
    pub fn blocking_completed(&self) -> usize {
        self.blocking_completed
    }
    /// Queued captures or abandoned results being destroyed on native workers.
    pub fn blocking_discarding(&self) -> usize {
        self.blocking_discarding
    }
    /// Maximum queued, running, completed, and discarding native jobs combined.
    pub fn blocking_capacity(&self) -> usize {
        self.blocking_capacity
    }
    /// Native body or late-result/queued-capture destructor panics observed by workers.
    pub fn blocking_panics(&self) -> u64 {
        self.blocking_panics
    }
    /// A native worker failed outside its ordinary job boundary; admission is closed.
    pub fn blocking_failed(&self) -> bool {
        self.blocking_failed
    }
}
