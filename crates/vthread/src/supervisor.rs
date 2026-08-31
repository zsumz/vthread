//! Explicit ownership of long-lived work independent of a lexical runtime scope.

use crate::{
    CancellationToken, Error, JoinHandle, Result, Runtime, ScopeOptions, TaskFailure, context,
};
use std::{marker::PhantomData, rc::Rc, time::Instant};

/// Cumulative outcomes observed after reclaiming the owned work.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShutdownReport {
    /// Bounded terminal component failures; an empty list is required for success.
    pub(crate) failures: crate::ThreadFailures,
    /// Tasks whose functions returned, including user-returned Result errors.
    pub(crate) completed: u64,
    /// Tasks that panicked.
    pub(crate) panicked: u64,
    /// Stacks or start packets reclaimed without normal completion.
    pub(crate) aborted: u64,
    /// Carrier failures observed by the runtime.
    pub(crate) failed_carriers: usize,
}

/// Deadline-bounded observation of supervised child reclamation, not runtime shutdown.
#[derive(Clone, Debug)]
#[non_exhaustive]
#[must_use = "Inspect timeout diagnostics and retain the owner until shutdown completes"]
pub enum SupervisorShutdownOutcome {
    /// This supervisor's child stacks were reclaimed; native services remain runtime-owned.
    Complete(ShutdownReport),
    /// The supervisor still owns its unfinished work and may be waited on again.
    TimedOut(Box<crate::RuntimeSnapshot>),
}

/// An ordinary-OS-caller owner for intentionally long-lived tasks.
///
/// Drop stops and reclaims its work and may block indefinitely on native calls, CPU loops
/// or destructors. Use request_shutdown and shutdown_until for a bounded observation, then
/// keep the owner until completion. It is non-Send and cannot be created on managed workers.
///
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<vthread::Supervisor<'static>>();
/// ```
#[must_use = "the supervisor owns child reclamation; dropping it can block"]
pub struct Supervisor<'runtime> {
    runtime: &'runtime Runtime,
    scope: Option<u64>,
    id: crate::diagnostics::ScopeId,
    cancellation: CancellationToken,
    completed: Option<ShutdownReport>,
    owner: PhantomData<Rc<()>>,
}

impl Runtime {
    /// Starts an explicit supervisor alongside the normal lexical scope.
    pub fn supervisor(&self, options: ScopeOptions) -> Result<Supervisor<'_>> {
        if context::current().is_some() {
            return Err(Error::InsideVThread);
        }
        if crate::worker_context::is_managed() {
            return Err(Error::InsideManagedWorker);
        }
        let scope = self.shared.begin_owned(options, true)?;
        let cancellation = self
            .shared
            .scope_options(scope)
            .expect("owned scope")
            .cancellation;
        Ok(Supervisor {
            runtime: self,
            scope: Some(scope),
            id: crate::diagnostics::ScopeId::new(scope),
            cancellation,
            completed: None,
            owner: PhantomData,
        })
    }
}

impl Supervisor<'_> {
    /// Stable runtime-local identity, retained after supervised work is reclaimed.
    /// Match this against [`crate::diagnostics::TaskSnapshot::scope`] to identify
    /// this supervisor's tasks in a runtime-wide timeout snapshot.
    pub fn id(&self) -> crate::diagnostics::ScopeId {
        self.id
    }

    /// Admits a transferable task owned by this supervisor.
    pub fn spawn<T: Send + 'static>(
        &self,
        name: impl Into<String>,
        entry: impl FnOnce() -> T + Send + 'static,
    ) -> Result<JoinHandle<T>> {
        self.runtime
            .spawn(self.scope.ok_or(Error::RuntimeStopped)?, name.into(), entry)
    }

    /// Returns the cancellation token inherited by supervised work.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    /// Requests cooperative cancellation without detaching or forcibly interrupting work.
    pub fn cancel(&self) {
        self.cancellation_token().cancel();
    }

    /// Stops this supervisor's work and waits for stack reclamation at runtime boundaries.
    pub fn shutdown(mut self) -> Result<ShutdownReport> {
        self.close(None)?.ok_or(Error::fault(
            crate::error::FaultComponent::Lifecycle,
            "unbounded supervisor wait timed out",
        ))
    }

    /// Closes child admission and requests reclamation without waiting for user work.
    pub fn request_shutdown(&self) {
        if let Some(scope) = self.scope {
            self.runtime
                .shared
                .abort_scope(scope, TaskFailure::SupervisorStopped);
        }
    }

    /// Waits until a monotonic deadline, retaining ownership on timeout for retry.
    /// Dropping this owner after a timeout still blocks until its children are reclaimed.
    /// Timeout snapshots include all runtime tasks; filter `snapshot.tasks()` using
    /// `task.scope() == self.id()` to select this supervisor's tasks.
    pub fn shutdown_until(&mut self, deadline: Instant) -> Result<SupervisorShutdownOutcome> {
        match self.close(Some(deadline))? {
            Some(report) => Ok(SupervisorShutdownOutcome::Complete(report)),
            None => Ok(SupervisorShutdownOutcome::TimedOut(Box::new(
                self.runtime.snapshot(),
            ))),
        }
    }

    fn close(&mut self, deadline: Option<Instant>) -> Result<Option<ShutdownReport>> {
        let Some(scope) = self.scope else {
            return Ok(self.completed.clone());
        };
        self.request_shutdown();
        if !self.runtime.shared.wait_until(scope, None, deadline)? {
            return Ok(None);
        }
        let report = self.runtime.shared.scope_report(scope);
        self.runtime.shared.finish_scope(scope);
        self.scope = None;
        self.completed = Some(report.clone());
        Ok(Some(report))
    }
}

impl Drop for Supervisor<'_> {
    fn drop(&mut self) {
        let _ = self.close(None);
    }
}

#[cfg(test)]
#[path = "supervisor_test.rs"]
mod supervisor_test;

impl ShutdownReport {
    /// Bounded terminal component failures; an empty list is required for success.
    pub fn failures(&self) -> &crate::ThreadFailures {
        &self.failures
    }
    /// Tasks whose functions returned, including user-returned Result errors.
    pub fn completed(&self) -> u64 {
        self.completed
    }
    /// Tasks that panicked.
    pub fn panicked(&self) -> u64 {
        self.panicked
    }
    /// Stacks or start packets reclaimed without normal completion.
    pub fn aborted(&self) -> u64 {
        self.aborted
    }
    /// Carrier failures observed by the runtime.
    pub fn failed_carriers(&self) -> usize {
        self.failed_carriers
    }
}
