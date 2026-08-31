//! Runtime ownership of the readiness driver and explicit blocking workers.

use crate::{Result, RuntimeConfig, blocking::pool::Pool, readiness::Reactor};

/// Bounded I/O and delegated-work activity for operator diagnostics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServiceSnapshot {
    /// Outstanding readiness waits, including not-yet-installed registrations.
    pub readiness_waits: usize,
    /// Configured readiness wait limit.
    pub readiness_capacity: usize,
    /// Registrations currently retained by zio.
    pub readiness_registered: usize,
    /// Whether the readiness driver failed and rejected further waits.
    pub readiness_failed: bool,
    /// First terminal backend failure, when present.
    pub readiness_error: Option<String>,
    /// Native jobs waiting for a worker.
    pub blocking_queued: usize,
    /// Native jobs already executing; these cannot be forcibly cancelled.
    pub blocking_running: usize,
    /// Stopped queued jobs whose capture destructors are executing on native workers.
    pub blocking_discarding: usize,
    /// Maximum queued, running, and discarding native jobs combined.
    pub blocking_capacity: usize,
    /// Native body or late-result/queued-capture destructor panics observed by workers.
    pub blocking_panics: u64,
    /// A native worker failed outside its ordinary job boundary; admission is closed.
    pub blocking_failed: bool,
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
