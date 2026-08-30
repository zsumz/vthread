//! Stop requests and deadline-bounded observation without detaching owned work.

use super::Runtime;
use crate::{Error, Result, RuntimeSnapshot, ShutdownPhase, ShutdownReport, context};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex},
    thread,
    time::Instant,
};

#[derive(Default)]
pub(super) struct ShutdownDriver {
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}

/// Result of a deadline-based runtime shutdown wait.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShutdownOutcome {
    /// All carrier and service threads have been joined and their resources reclaimed.
    Complete(ShutdownReport),
    /// Work remains runtime-owned; the deadline expired without detaching any thread.
    TimedOut(Box<RuntimeSnapshot>),
}

impl Runtime {
    /// Stops admission and requests cancellation without joining or running user destructors.
    ///
    /// Idempotent and safe to request from a virtual thread or native worker. A request
    /// is not completion: ordinary OS callers use `shutdown` or `shutdown_until` to wait.
    /// Internal metadata locks and wake delivery are not hard real-time operations.
    pub fn request_shutdown(&self) {
        self.shared.request_stop();
    }

    /// Stops admission, reclaims tasks, and joins carriers and native services.
    ///
    /// This cannot preempt CPU loops, native blocking calls, FFI, or destructors. Such
    /// work may delay shutdown indefinitely. Virtual threads and this runtime's own
    /// native workers cannot wait for shutdown; that would require joining their own work.
    pub fn shutdown(&self) -> Result<ShutdownReport> {
        self.check_shutdown_caller()?;
        self.request_shutdown();
        self.join_workers();
        self.drain_services();
        self.shared.advance_shutdown(ShutdownPhase::Complete);
        Ok(self.shutdown_report())
    }

    /// Requests shutdown and waits up to an absolute monotonic deadline.
    ///
    /// An expired deadline still checks completion once. Timeout retains every unfinished
    /// thread under runtime ownership; callers can inspect the snapshot and retry. Dropping
    /// the runtime still drains work and may block indefinitely. This method never waits
    /// behind another caller's blocking join. One lazily started, runtime-owned coordinator
    /// performs joins (including OS TLS cleanup). OS scheduling, coordinator startup, and
    /// metadata operations are not a hard real-time guarantee. Calls from virtual threads or this runtime's native
    /// workers fail before admission changes; those callers may use `request_shutdown`.
    ///
    /// ```
    /// use std::time::{Duration, Instant};
    /// use vthread::{Runtime, ShutdownOutcome};
    /// let runtime = Runtime::new()?;
    /// match runtime.shutdown_until(Instant::now() + Duration::from_secs(1))? {
    ///     ShutdownOutcome::Complete(report) => assert_eq!(report.failed_carriers, 0),
    ///     ShutdownOutcome::TimedOut(snapshot) => assert!(!snapshot.accepting),
    /// }
    /// # Ok::<(), vthread::Error>(())
    /// ```
    pub fn shutdown_until(&self, deadline: Instant) -> Result<ShutdownOutcome> {
        self.check_shutdown_caller()?;
        self.request_shutdown();
        self.start_shutdown_driver()?;
        loop {
            let observed = self.shared.changed.version();
            match self.shared.shutdown_phase() {
                ShutdownPhase::Complete => {
                    return Ok(ShutdownOutcome::Complete(self.shutdown_report()));
                }
                ShutdownPhase::Failed => {
                    return Err(Error::Invariant("shutdown coordinator failed"));
                }
                _ => {}
            }
            if Instant::now() >= deadline {
                return Ok(ShutdownOutcome::TimedOut(Box::new(self.snapshot())));
            }
            self.shared.changed.wait(observed, Some(deadline));
        }
    }

    pub(super) fn start_shutdown_driver(&self) -> Result<()> {
        let mut worker = crate::signal::lock(&self.shutdown_driver.worker);
        if worker.is_some() || self.shared.shutdown_phase() == ShutdownPhase::Complete {
            return Ok(());
        }
        let shared = Arc::clone(&self.shared);
        let workers = Arc::clone(&self.workers);
        *worker = Some(
            thread::Builder::new()
                .name("vthread-shutdown".to_owned())
                .stack_size(256 * 1024)
                .spawn(move || {
                    // JoinHandle::is_finished may precede OS TLS destructors. Only a real join
                    // proves thread reclamation; timed callers must never perform that join.
                    let outcome = catch_unwind(AssertUnwindSafe(|| {
                        shared.advance_shutdown(ShutdownPhase::JoiningCarriers);
                        for worker in crate::signal::lock(&workers).drain(..) {
                            let _ = worker.join();
                        }
                        drain_services(&shared);
                    }));
                    shared.advance_shutdown(if outcome.is_ok() {
                        ShutdownPhase::Complete
                    } else {
                        ShutdownPhase::Failed
                    });
                })?,
        );
        Ok(())
    }

    pub(super) fn owns_current_worker(&self) -> bool {
        self.worker_ids.contains(&thread::current().id())
            || self
                .shared
                .services
                .get()
                .is_some_and(|services| services.blocking.owns_current_thread())
    }

    pub(super) fn drain_services(&self) {
        drain_services(&self.shared);
    }

    pub(super) fn join_shutdown_driver(&self) {
        let worker = crate::signal::lock(&self.shutdown_driver.worker).take();
        if let Some(worker) = worker {
            let _ = worker.join();
        }
    }

    fn check_shutdown_caller(&self) -> Result<()> {
        if context::current().is_some() {
            return Err(Error::InsideVThread);
        }
        if self
            .shared
            .services
            .get()
            .is_some_and(|services| services.blocking.owns_current_thread())
        {
            return Err(Error::InsideBlockingWorker);
        }
        Ok(())
    }

    fn shutdown_report(&self) -> ShutdownReport {
        let snapshot = self.snapshot();
        ShutdownReport {
            completed: snapshot.stats.completed,
            panicked: snapshot.stats.panicked,
            aborted: snapshot.stats.aborted,
            failed_carriers: snapshot
                .carriers
                .iter()
                .filter(|carrier| carrier.status == crate::CarrierStatus::Failed)
                .count(),
        }
    }
}

fn drain_services(shared: &crate::control::Shared) {
    if let Some(services) = shared.services.get() {
        shared.advance_shutdown(ShutdownPhase::JoiningReadiness);
        services.reactor.join();
        shared.advance_shutdown(ShutdownPhase::JoiningNative);
        services.blocking.join();
    }
}

#[cfg(test)]
#[path = "runtime_lifecycle_test.rs"]
mod runtime_lifecycle_test;
