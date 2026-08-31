//! Stop requests and deadline-bounded observation of a single owned join operation.

use super::Runtime;
use crate::{
    Error, Result, RuntimeSnapshot, ShutdownPhase, ShutdownReport, context, control::Shared,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

pub(super) struct ShutdownDriver {
    ready: Arc<AtomicBool>,
}

impl ShutdownDriver {
    pub(super) fn new(shared: &Arc<Shared>) -> Result<Self> {
        let ready = Arc::new(AtomicBool::new(false));
        let constructed = Arc::clone(&ready);
        let shared = Arc::clone(shared);
        let resources = Arc::clone(&shared.resources);
        let cleanup = Arc::clone(&resources);
        crate::lifecycle_owner::start(Arc::clone(&shared), resources, move || {
            loop {
                let observed = shared.changed.version();
                if constructed.load(Ordering::Acquire)
                    && shared.shutdown_phase() != ShutdownPhase::NotRequested
                {
                    break;
                }
                shared.changed.wait(observed, None);
            }
            #[cfg(test)]
            assert!(
                !shared.fail_coordinator_before_drain.load(Ordering::Relaxed),
                "injected coordinator failure before cleanup"
            );
            shared.advance_shutdown(ShutdownPhase::JoiningCarriers);
            cleanup
                .workers
                .join_all(&Arc::downgrade(&shared), crate::ThreadComponent::Carrier);
            // A retained affine stack may own a native-result acknowledgement. Joining
            // services then could wait forever; let the process owner fail stop now.
            if !cleanup.carriers_reclaimed(&shared) {
                return;
            }
            drain_services(&shared);
            #[cfg(test)]
            if let Some(hook) = crate::signal::lock(&shared.coordinator_exit_hook).take() {
                hook();
            }
            // Only the process owner may publish completion, after joining this thread.
        })?;
        Ok(Self { ready })
    }

    pub(super) fn ready(&self, shared: &Shared) {
        self.ready.store(true, Ordering::Release);
        shared.changed.notify();
    }
}

/// Result of a deadline-based runtime shutdown wait.
#[derive(Clone, Debug)]
#[non_exhaustive]
#[must_use = "Inspect timeout diagnostics and retain the owner until shutdown completes"]
pub enum ShutdownOutcome {
    /// Every carrier, service and coordinator has been joined, including OS TLS cleanup.
    Complete(ShutdownReport),
    /// Work remains runtime-owned; the deadline expired without detaching any thread.
    TimedOut(Box<RuntimeSnapshot>),
}

impl Runtime {
    /// Stops admission and requests cancellation without joining or running user destructors.
    ///
    /// Idempotent and safe from managed workers. A request is not completion: ordinary
    /// OS callers use `shutdown` or `shutdown_until` to wait. Metadata locks and wake
    /// delivery are not hard real-time operations.
    pub fn request_shutdown(&self) {
        self.shared.request_stop();
    }

    /// Stops admission, reclaims tasks, and waits for the single owned join operation.
    ///
    /// CPU loops, native calls, FFI, and destructors can delay completion indefinitely.
    /// No virtual thread or managed native worker may wait, including foreign workers.
    /// A failed process lifecycle service returns `Error::LifecycleFailed` promptly;
    /// unfinished coordinators stay process-owned and cleanup is not claimed complete.
    pub fn shutdown(&self) -> Result<ShutdownReport> {
        self.check_shutdown_caller()?;
        self.request_shutdown();
        self.wait_shutdown(None)?;
        Ok(self.shutdown_report())
    }

    /// Requests shutdown and observes completion up to an absolute monotonic deadline.
    ///
    /// An expired deadline checks completion once. Timeout retains every unfinished
    /// thread under the process lifecycle owner's bounded ownership; callers can retry.
    /// One coordinator is established before runtime workers start. Final-owner Drop on
    /// an ordinary OS thread waits; on any managed worker it only requests shutdown.
    /// The process service retains and joins the coordinator, including OS TLS cleanup.
    /// Service failure returns `Error::LifecycleFailed` without waiting for the deadline.
    /// Scheduling and metadata locks are not a hard real-time guarantee.
    ///
    /// ```
    /// use std::time::{Duration, Instant};
    /// use vthread::{Runtime, lifecycle::ShutdownOutcome};
    /// let runtime = Runtime::new()?;
    /// match runtime.shutdown_until(Instant::now() + Duration::from_secs(1))? {
    ///     ShutdownOutcome::Complete(report) => assert_eq!(report.failed_carriers(), 0),
    ///     ShutdownOutcome::TimedOut(snapshot) => assert!(!snapshot.accepting()),
    ///     _ => runtime.shutdown().map(|_| ())?,
    /// }
    /// # Ok::<(), vthread::Error>(())
    /// ```
    pub fn shutdown_until(&self, deadline: Instant) -> Result<ShutdownOutcome> {
        self.check_shutdown_caller()?;
        self.request_shutdown();
        if self.wait_shutdown(Some(deadline))? {
            Ok(ShutdownOutcome::Complete(self.shutdown_report()))
        } else {
            Ok(ShutdownOutcome::TimedOut(Box::new(self.snapshot())))
        }
    }

    pub(super) fn wait_shutdown(&self, deadline: Option<Instant>) -> Result<bool> {
        loop {
            let observed = self.shared.changed.version();
            match self.shared.shutdown_phase() {
                ShutdownPhase::Complete => return Ok(true),
                ShutdownPhase::Failed => {
                    return Err(Error::ShutdownFailed(Box::new(self.shutdown_report())));
                }
                _ => {}
            }
            crate::lifecycle_owner::check_health()?;
            if deadline.is_some_and(|end| Instant::now() >= end) {
                return Ok(false);
            }
            self.shared.changed.wait(observed, deadline);
        }
    }

    fn check_shutdown_caller(&self) -> Result<()> {
        if context::current().is_some() {
            return Err(Error::InsideVThread);
        }
        if crate::worker_context::is_managed() {
            return Err(Error::InsideManagedWorker);
        }
        Ok(())
    }

    fn shutdown_report(&self) -> ShutdownReport {
        let snapshot = self.snapshot();
        ShutdownReport {
            failures: snapshot.failures,
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

fn drain_services(shared: &Shared) {
    if let Some(services) = shared.services.get() {
        // A stop request can race publication during runtime construction.
        services.stop();
        shared.advance_shutdown(ShutdownPhase::JoiningReadiness);
        services.reactor.join();
        shared.advance_shutdown(ShutdownPhase::JoiningNative);
        services.blocking.join();
    }
}

#[cfg(test)]
#[path = "runtime_lifecycle_test.rs"]
mod runtime_lifecycle_test;

#[cfg(test)]
#[path = "runtime_ownership_test.rs"]
mod runtime_ownership_test;
