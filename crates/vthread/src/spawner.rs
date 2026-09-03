//! Transferable admission capabilities; structured owners retain all child lifetimes.

use crate::{
    Error, JoinHandle, Result, SpawnOptions, context, control::Shared, options::SpawnParent,
};
use std::sync::{Arc, Weak};

/// A cloneable, Send + Sync capability to spawn children of one scope or supervisor.
///
/// This is not an owner and does not keep the runtime or a finished scope alive.
/// Owner exit closes admission before draining; retained capabilities then return
/// [`Error::ScopeClosed`]. Keep a root callback open while dynamic admissions are needed,
/// for example by joining its accept loop. Dropping child handles never detaches work.
///
/// A caller on this runtime becomes the diagnostic parent. Its cancellation and deadline
/// are inherited in addition to the target owner's policy. Ordinary OS callers and tasks
/// on another runtime inherit only the target owner's policy, without a diagnostic parent.
/// Started children never migrate; their initial placement uses normal carrier admission.
#[derive(Clone, Debug)]
/// Transferable children cannot borrow the caller's stack:
/// ```compile_fail
/// fn borrowed(spawner: &vthread::Spawner) {
///     let name = String::from("borrowed");
///     spawner.spawn("child", || name.len()).unwrap();
/// }
/// ```
/// Their results must also be transferable:
/// ```compile_fail
/// fn local_result(spawner: &vthread::Spawner) {
///     spawner.spawn("child", || std::rc::Rc::new(42)).unwrap();
/// }
/// ```
pub struct Spawner {
    shared: Weak<Shared>,
    scope: u64,
}

impl Spawner {
    pub(crate) fn new(shared: &Arc<Shared>, scope: u64) -> Self {
        Self {
            shared: Arc::downgrade(shared),
            scope,
        }
    }

    /// Spawns a named, transferable child owned by the original structured owner.
    pub fn spawn<T: Send + 'static>(
        &self,
        name: impl Into<String>,
        entry: impl FnOnce() -> T + Send + 'static,
    ) -> Result<JoinHandle<T>> {
        self.spawn_with(SpawnOptions::default(), name, entry)
    }

    /// Spawns with a deadline no later than the owner's and same-runtime caller's.
    /// Rejection consumes the closure; captures are dropped on the caller outside locks.
    pub fn spawn_with<T: Send + 'static>(
        &self,
        options: SpawnOptions,
        name: impl Into<String>,
        entry: impl FnOnce() -> T + Send + 'static,
    ) -> Result<JoinHandle<T>> {
        let shared = self.shared.upgrade().ok_or(Error::ScopeClosed)?;
        Self::spawn_on(shared, self.scope, options, name, entry)
    }

    pub(crate) fn spawn_on<T: Send + 'static>(
        shared: Arc<Shared>,
        scope: u64,
        options: SpawnOptions,
        name: impl Into<String>,
        entry: impl FnOnce() -> T + Send + 'static,
    ) -> Result<JoinHandle<T>> {
        let parent = if let Some(mounted) = context::current() {
            let execution = mounted.execution()?;
            execution.data.check()?;
            Arc::ptr_eq(&shared, execution.shared()).then(|| {
                let record = execution.record().lock();
                SpawnParent {
                    id: record.id,
                    scope: record.scope,
                    options: execution.data.options(),
                }
            })
        } else {
            None
        };
        let spawned = shared.submit_with(scope, options, name.into(), entry, parent)?;
        Ok(JoinHandle::new(
            shared,
            spawned.id,
            spawned.cell,
            spawned.record,
        ))
    }
}

#[cfg(test)]
#[path = "spawner_test.rs"]
mod spawner_test;

#[cfg(test)]
#[path = "spawner_admission_test.rs"]
mod spawner_admission_test;

#[cfg(test)]
#[path = "spawner_policy_test.rs"]
mod spawner_policy_test;
