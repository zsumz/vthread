//! Borrowed scope ownership on the currently mounted task's carrier.

use crate::{
    CancellationToken, Error, LocalJoinHandle, Result, SuspensionReason, context::Execution,
    join::JoinCell, join_wait, kernel::Task, options::TaskOptions, signal::lock,
    task::SharedTaskRecord, task_context::TaskContext, task_fiber::TaskFiber,
};
use std::{cell::RefCell, marker::PhantomData, rc::Rc, sync::Arc, time::Instant};
use vthread_stack::FiberScope;

#[path = "local_scope_run.rs"]
mod local_scope_run;
pub use local_scope_run::{
    local_scope, local_scope_with_deadline, try_local_scope, try_local_scope_with_deadline,
};

/// A lexical owner of borrowed, non-Send children on the current carrier.
pub struct LocalScope<'scope, 'env: 'scope> {
    fibers: &'scope FiberScope<'scope, 'env>,
    execution: Execution,
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
        self.execution.data.check()?;
        self.options.check()?;
        #[cfg(feature = "runtime-evidence")]
        if let Err(error) = self.execution.local.check_capacity() {
            self.execution.shared.record_admission_rejected(
                crate::error::CapacityResource::CarrierQueue,
                self.execution.shared.config.carrier_queue_capacity(),
            );
            return Err(error);
        }
        #[cfg(not(feature = "runtime-evidence"))]
        self.execution.local.check_capacity()?;
        let (root, parent, carrier) = {
            let record = lock(&self.execution.record);
            (record.scope, record.id, record.carrier)
        };
        let record = self.execution.shared.reserve(
            root,
            name.into(),
            Some((carrier, parent, self.options.child(options.deadline))),
        )?;
        #[cfg(feature = "runtime-evidence")]
        let acquired = self
            .execution
            .local
            .stacks
            .borrow_mut()
            .acquire_identified();
        #[cfg(not(feature = "runtime-evidence"))]
        let acquired = self.execution.local.stacks.borrow_mut().acquire();
        #[cfg(feature = "runtime-evidence")]
        let (stack_identity, stack) = match acquired {
            Ok(stack) => stack,
            Err(error) => {
                self.execution.shared.release_reservation(&record);
                return Err(Error::StackAllocation(error));
            }
        };
        #[cfg(not(feature = "runtime-evidence"))]
        let stack = match acquired {
            Ok(stack) => stack,
            Err(error) => {
                self.execution.shared.release_reservation(&record);
                return Err(Error::StackAllocation(error));
            }
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
            Err(error) => {
                #[cfg(feature = "runtime-evidence")]
                self.execution
                    .local
                    .stacks
                    .borrow_mut()
                    .retire(stack_identity);
                self.execution.shared.release_reservation(&record);
                return Err(Error::StackAllocation(error));
            }
        };
        let data = Rc::new(TaskContext::new(
            lock(&record).options.clone(),
            self.execution.shared.config.task_local_capacity(),
        ));
        let cleanup = Execution {
            record: Arc::clone(&record),
            data: Rc::clone(&data),
            shared: Arc::clone(&self.execution.shared),
            local: Rc::clone(&self.execution.local),
        };
        let hub = Arc::clone(&self.execution.shared.inboxes[carrier.0].hub);
        lease.cleanup_context(move || {
            Box::new(crate::task_context::TaskCleanup::new(
                cleanup.clone(),
                Arc::clone(&hub),
            ))
        });
        self.records.borrow_mut().retain(|record| {
            let record = lock(record);
            !(record.status.is_terminal() && record.outcome_observed)
        });
        self.records.borrow_mut().push(Arc::clone(&record));
        #[cfg(feature = "runtime-evidence")]
        let task_fiber = TaskFiber::borrowed(lease, stack_identity);
        #[cfg(not(feature = "runtime-evidence"))]
        let task_fiber = TaskFiber::borrowed(lease);
        self.execution.local.starts.borrow_mut().push_back(Task {
            record: Arc::clone(&record),
            data,
            fiber: Some(task_fiber),
        });
        #[cfg(feature = "runtime-evidence")]
        {
            self.execution.shared.record_task_accepted(&record);
            self.execution.shared.record(
                crate::diagnostics::evidence::RuntimeEventKind::StackCheckedOut {
                    task: lock(&record).id,
                    stack: crate::diagnostics::evidence::StackId::new(carrier, stack_identity),
                },
            );
            self.execution.shared.record(
                crate::diagnostics::evidence::RuntimeEventKind::QueueDepth {
                    carrier,
                    queue: crate::diagnostics::evidence::QueueKind::LocalStart,
                    depth: self.execution.local.starts.borrow().len(),
                    capacity: self.execution.shared.config.carrier_queue_capacity(),
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
            let mut record = lock(&record);
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
            lock(record).outcome_observed = true;
        }
    }
}

#[cfg(test)]
#[path = "local_scope_test.rs"]
mod local_scope_test;
