//! Borrowed scope ownership on the currently mounted task's carrier.

use crate::{
    CancellationToken, Error, LocalJoinHandle, Result, SuspensionReason, context::Execution,
    join::JoinCell, join_wait, kernel_tasks::BorrowedTask, options::TaskOptions,
    task::SharedTaskRecord, task_context::TaskContext, task_fiber::BorrowedFiber,
};
use std::{cell::RefCell, marker::PhantomData, rc::Rc, sync::Arc, time::Instant};
use vthread_stack::{FiberLease, FiberScope};

#[cfg(test)]
thread_local! {
    static INJECT_CONSTRUCTION_PANIC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn inject_construction_panic() {
    INJECT_CONSTRUCTION_PANIC.with(|injected| injected.set(true));
}

#[cfg(test)]
fn construction_boundary() {
    INJECT_CONSTRUCTION_PANIC.with(|injected| {
        assert!(
            !injected.replace(false),
            "injected local construction panic"
        );
    });
}

struct LocalAdmissionRollback<'a> {
    execution: &'a Execution,
    records: &'a RefCell<Vec<SharedTaskRecord>>,
    record: &'a SharedTaskRecord,
    fiber: Option<FiberLease>,
    listed: bool,
    #[cfg(feature = "runtime-evidence")]
    stack: Option<u64>,
    armed: bool,
}

impl LocalAdmissionRollback<'_> {
    fn retain_fiber(&mut self, fiber: &FiberLease) {
        self.fiber = Some(fiber.clone());
    }

    fn retain_record(&mut self) {
        self.listed = true;
    }

    fn commit(mut self) {
        self.armed = false;
    }
}

impl Drop for LocalAdmissionRollback<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Some(fiber) = self.fiber.take()
            && let Err(payload) =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fiber.reclaim()))
        {
            crate::worker_context::payload_failure(crate::PanicReport::capture(payload));
        }
        #[cfg(feature = "runtime-evidence")]
        if let Some(stack) = self.stack.take() {
            self.execution.local().stacks.borrow_mut().retire(stack);
        }
        if self.listed {
            self.records
                .borrow_mut()
                .retain(|record| !Arc::ptr_eq(record, self.record));
        }
        self.execution.shared().release_reservation(self.record);
    }
}

#[path = "local_scope_run.rs"]
mod local_scope_run;
pub use local_scope_run::{
    local_scope, local_scope_with_deadline, try_local_scope, try_local_scope_with_deadline,
};

/// A lexical owner of borrowed, non-Send children on the current carrier.
pub struct LocalScope<'scope, 'env: 'scope> {
    fibers: &'scope FiberScope<'scope, 'env>,
    execution: Rc<Execution>,
    options: TaskOptions,
    records: RefCell<Vec<SharedTaskRecord>>,
}

impl<'scope, 'env> LocalScope<'scope, 'env> {
    /// Spawns borrowed work without moving it to another carrier.
    pub fn spawn<T: 'scope>(
        &self,
        name: impl Into<String>,
        entry: impl FnOnce() -> T + 'scope,
    ) -> Result<LocalJoinHandle<'scope, T>> {
        self.spawn_with(crate::SpawnOptions::default(), name, entry)
    }

    /// Spawns borrowed work with a deadline no later than the local group's deadline.
    pub fn spawn_with<T: 'scope>(
        &self,
        options: crate::SpawnOptions,
        name: impl Into<String>,
        entry: impl FnOnce() -> T + 'scope,
    ) -> Result<LocalJoinHandle<'scope, T>> {
        // Name conversion is user code and may reenter this scope, so it precedes final checks.
        let name = name.into();
        self.execution.data.check()?;
        self.options.check()?;
        #[cfg(feature = "runtime-evidence")]
        if let Err(error) = self.execution.local().check_capacity() {
            self.execution.shared().record_admission_rejected(
                crate::error::CapacityResource::CarrierQueue,
                self.execution.shared().config.carrier_queue_capacity(),
            );
            return Err(error);
        }
        #[cfg(not(feature = "runtime-evidence"))]
        self.execution.local().check_capacity()?;
        let (root, parent, carrier) = {
            let record = self.execution.record().lock();
            (record.scope, record.id, record.carrier)
        };
        let record = self.execution.shared().reserve(
            root,
            name,
            Some((carrier, parent, self.options.child(options.deadline))),
        )?;
        let mut rollback = LocalAdmissionRollback {
            execution: &self.execution,
            records: &self.records,
            record: &record,
            fiber: None,
            listed: false,
            #[cfg(feature = "runtime-evidence")]
            stack: None,
            armed: true,
        };
        #[cfg(feature = "runtime-evidence")]
        let acquired = self
            .execution
            .local()
            .stacks
            .borrow_mut()
            .acquire_identified();
        #[cfg(not(feature = "runtime-evidence"))]
        let acquired = self.execution.local().stacks.borrow_mut().acquire();
        #[cfg(feature = "runtime-evidence")]
        let (stack_identity, stack) = match acquired {
            Ok(stack) => stack,
            Err(error) => return Err(Error::StackAllocation(error)),
        };
        #[cfg(feature = "runtime-evidence")]
        {
            rollback.stack = Some(stack_identity);
        }
        #[cfg(not(feature = "runtime-evidence"))]
        let stack = match acquired {
            Ok(stack) => stack,
            Err(error) => return Err(Error::StackAllocation(error)),
        };
        let cell = Rc::new(RefCell::new(JoinCell { outcome: None }));
        let body_cell = Rc::clone(&cell);
        let body_record = Arc::clone(&record);
        let lease = match self.fibers.spawn(stack, move || {
            crate::task_body::run(&body_record, entry, move |outcome| {
                body_cell.borrow_mut().outcome = Some(outcome);
            });
        }) {
            Ok(lease) => lease,
            Err(error) => return Err(Error::StackAllocation(error)),
        };
        rollback.retain_fiber(&lease);
        #[cfg(test)]
        construction_boundary();
        let data = Rc::new(TaskContext::new(
            record.lock().options().clone(),
            self.execution.shared().config.task_local_capacity(),
        ));
        let (id, root) = {
            let record = record.lock();
            (record.id, record.scope)
        };
        let execution = Rc::new(Execution::new(
            id,
            root,
            Arc::clone(self.execution.hub()),
            Arc::clone(&record),
            Arc::clone(self.execution.shared()),
            Rc::clone(self.execution.local()),
            Rc::clone(&data),
        ));
        let cleanup = Rc::clone(&execution);
        lease.cleanup_context(move || {
            Box::new(crate::task_context::TaskCleanup::new(Rc::clone(&cleanup)))
        });
        self.records.borrow_mut().retain(|record| {
            let record = record.lock();
            !(record.status.is_terminal() && record.outcome_observed)
        });
        self.records.borrow_mut().push(Arc::clone(&record));
        rollback.retain_record();
        #[cfg(feature = "runtime-evidence")]
        let task_fiber = BorrowedFiber::new(lease, stack_identity);
        #[cfg(not(feature = "runtime-evidence"))]
        let task_fiber = BorrowedFiber::new(lease);
        self.execution.local().push_start(BorrowedTask {
            execution: Some(execution),
            fiber: Some(task_fiber),
        });
        rollback.commit();
        #[cfg(feature = "runtime-evidence")]
        {
            self.execution.shared().record_task_accepted(&record);
            self.execution.shared().record(
                crate::diagnostics::evidence::RuntimeEventKind::StackCheckedOut {
                    task: record.lock().id,
                    stack: crate::diagnostics::evidence::StackId::new(carrier, stack_identity),
                },
            );
            self.execution.shared().record(
                crate::diagnostics::evidence::RuntimeEventKind::QueueDepth {
                    carrier,
                    queue: crate::diagnostics::evidence::QueueKind::LocalStart,
                    depth: self.execution.local().pending_starts(),
                    capacity: self.execution.shared().config.carrier_queue_capacity(),
                },
            );
        }
        Ok(LocalJoinHandle {
            record,
            cell,
            lifetime: PhantomData,
            taken: false,
        })
    }

    /// Returns the cancellation token inherited by this scope's children.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.options.cancellation.clone()
    }
    /// Requests cooperative cancellation of this scope and its descendants.
    pub fn cancel(&self) {
        self.options.cancellation.cancel();
    }
    /// Returns the earliest parent or child deadline.
    pub fn deadline(&self) -> Option<Instant> {
        self.options.deadline
    }

    fn drain(&self) -> crate::ScopeFailure {
        let records = self.records.borrow().clone();
        let mut failure = crate::ScopeFailure::default();
        for record in records {
            if let Err(error) = join_wait::wait_for(&record, SuspensionReason::ScopeDrain, true) {
                failure.cleanup_failed(error);
            }
            let mut record = record.lock();
            if !record.outcome_observed {
                if let Some(reason) = record.failure {
                    failure.child_failed(Error::TaskAborted {
                        task: record.id,
                        reason,
                    });
                } else if let Some(panic) = &record.panic {
                    failure.child_failed(Error::task_panicked(
                        record.id,
                        record.name.to_string(),
                        panic.clone(),
                    ));
                }
            }
            record.outcome_observed = true;
        }
        failure
    }
}

impl Drop for LocalScope<'_, '_> {
    fn drop(&mut self) {
        // Descendant failures belong to this owner, including when the owner is
        // forcibly unwound. Do not report them again as unobserved root children.
        for record in self.records.get_mut() {
            record.lock().outcome_observed = true;
        }
    }
}

#[cfg(test)]
#[path = "local_scope_test.rs"]
mod local_scope_test;
