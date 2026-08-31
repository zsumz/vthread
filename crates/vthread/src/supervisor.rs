//! Explicit ownership of long-lived work independent of a lexical runtime scope.

use crate::{
    CancellationToken, Error, JoinHandle, Result, Runtime, ScopeOptions, TaskFailure, context,
};
use std::{marker::PhantomData, rc::Rc};

/// Cumulative outcomes observed after reclaiming the owned work.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShutdownReport {
    /// Bounded terminal component failures; an empty list is required for success.
    pub failures: crate::ThreadFailures,
    /// Tasks whose functions returned, including user-returned Result errors.
    pub completed: u64,
    /// Tasks that panicked.
    pub panicked: u64,
    /// Stacks or start packets reclaimed without normal completion.
    pub aborted: u64,
    /// Carrier failures observed by the runtime.
    pub failed_carriers: usize,
}

/// An ordinary-OS-caller owner for intentionally long-lived tasks.
///
/// Dropping this owner stops and reclaims its work. It is deliberately non-Send:
/// its blocking destructor must not move into a virtual thread.
///
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<vthread::Supervisor<'static>>();
/// ```
pub struct Supervisor<'runtime> {
    runtime: &'runtime Runtime,
    scope: Option<u64>,
    owner: PhantomData<Rc<()>>,
}

impl Runtime {
    /// Starts an explicit supervisor alongside the normal lexical scope.
    pub fn supervisor(&self, options: ScopeOptions) -> Result<Supervisor<'_>> {
        if context::current().is_some() {
            return Err(Error::InsideVThread);
        }
        let scope = self.shared.begin_owned(options, true)?;
        Ok(Supervisor {
            runtime: self,
            scope: Some(scope),
            owner: PhantomData,
        })
    }
}

impl Supervisor<'_> {
    /// Admits a transferable task owned by this supervisor.
    pub fn spawn<T: Send + 'static>(
        &self,
        name: impl Into<String>,
        entry: impl FnOnce() -> T + Send + 'static,
    ) -> Result<JoinHandle<T>> {
        self.runtime
            .spawn(self.scope.expect("live supervisor"), name.into(), entry)
    }

    /// Returns the cancellation token inherited by supervised work.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.runtime
            .shared
            .scope_options(self.scope.expect("live supervisor"))
            .expect("owned scope")
            .cancellation
    }

    /// Requests cooperative cancellation without detaching or forcibly interrupting work.
    pub fn cancel(&self) {
        self.cancellation_token().cancel();
    }

    /// Stops this supervisor's work and waits for stack reclamation at runtime boundaries.
    pub fn shutdown(mut self) -> Result<ShutdownReport> {
        self.close()
    }

    fn close(&mut self) -> Result<ShutdownReport> {
        let Some(scope) = self.scope.take() else {
            return Ok(ShutdownReport::default());
        };
        self.runtime
            .shared
            .abort_scope(scope, TaskFailure::SupervisorStopped);
        let drained = self.runtime.shared.wait(scope, None);
        let report = self.runtime.shared.scope_report(scope);
        self.runtime.shared.finish_scope(scope);
        drained?;
        Ok(report)
    }
}

impl Drop for Supervisor<'_> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
#[path = "supervisor_test.rs"]
mod supervisor_test;
