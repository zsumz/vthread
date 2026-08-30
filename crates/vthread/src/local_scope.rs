//! Borrowed scope ownership on the currently mounted task's carrier.

use crate::{
    CancellationToken, Error, LocalJoinHandle, Result, SuspensionReason,
    context::{self, Execution},
    join::JoinCell,
    join_wait,
    kernel::Task,
    options::TaskOptions,
    signal::lock,
    task::SharedTaskRecord,
    task_context::TaskContext,
    task_fiber::TaskFiber,
};
use std::{
    cell::RefCell,
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    rc::Rc,
    sync::Arc,
    time::Instant,
};
use vthread_stack::FiberScope;

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
        self.execution.data.check()?;
        self.options.check()?;
        self.execution.local.check_capacity()?;
        let (root, parent, carrier) = {
            let record = lock(&self.execution.record);
            (record.scope, record.id, record.carrier)
        };
        let record = self.execution.shared.reserve(
            root,
            name.into(),
            Some((carrier, parent, self.options.child(None))),
        )?;
        let acquired = self.execution.local.stacks.borrow_mut().acquire();
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
        self.execution.local.starts.borrow_mut().push_back(Task {
            record: Arc::clone(&record),
            data,
            fiber: Some(TaskFiber::Borrowed(lease)),
        });
        Ok(LocalJoinHandle {
            record,
            cell,
            lifetime: PhantomData,
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

    fn drain(&self) -> Result<()> {
        let records = self.records.borrow().clone();
        let mut failure = None;
        for record in records {
            if let Err(error) = join_wait::wait_for(&record, SuspensionReason::ScopeDrain, true) {
                failure.get_or_insert(error);
            }
            let mut record = lock(&record);
            if !record.outcome_observed {
                if let Some(reason) = record.failure {
                    failure.get_or_insert(Error::TaskAborted {
                        task: record.id,
                        reason,
                    });
                } else if let Some(panic) = &record.panic {
                    failure.get_or_insert_with(|| {
                        Error::task_panicked(record.id, record.name.to_string(), panic.clone())
                    });
                }
            }
            record.outcome_observed = true;
        }
        failure.map_or(Ok(()), Err)
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

/// Runs borrowed work on the current carrier, reclaiming all children before returning.
///
/// ```compile_fail
/// vthread::local_scope(|scope| Ok(scope.spawn("escape", || 1)?));
/// ```
///
/// ```compile_fail
/// vthread::local_scope(|scope| {
///     let too_short = String::from("scope body local");
///     scope.spawn("invalid borrow", || too_short.len())?;
///     Ok(())
/// });
/// ```
///
/// ```compile_fail
/// vthread::local_scope(|scope| {
///     scope.spawn("cannot retain the facade", || scope.cancel())?;
///     Ok(())
/// });
/// ```
pub fn local_scope<'env, R>(
    body: impl for<'scope> FnOnce(&LocalScope<'scope, 'env>) -> Result<R>,
) -> Result<R> {
    run_local(None, body)
}

/// Adds a local deadline; it cannot extend an inherited parent deadline.
pub fn local_scope_with_deadline<'env, R>(
    deadline: Instant,
    body: impl for<'scope> FnOnce(&LocalScope<'scope, 'env>) -> Result<R>,
) -> Result<R> {
    run_local(Some(deadline), body)
}

fn run_local<'env, R>(
    deadline: Option<Instant>,
    body: impl for<'scope> FnOnce(&LocalScope<'scope, 'env>) -> Result<R>,
) -> Result<R> {
    let mounted = context::current().ok_or(Error::OutsideVThread)?;
    let execution = mounted.execution()?.clone();
    execution.data.check()?;
    let options = execution.data.options.child(deadline);
    vthread_stack::fiber_scope(execution.shared.config.max_vthreads(), |fibers| {
        let scope = LocalScope {
            fibers,
            execution,
            options,
            records: RefCell::new(Vec::new()),
        };
        let outcome = catch_unwind(AssertUnwindSafe(|| body(&scope)));
        if !matches!(&outcome, Ok(Ok(_))) || scope.options.check().is_err() {
            scope.cancel();
        }
        let drained = scope.drain();
        match outcome {
            Err(payload) => resume_unwind(payload),
            Ok(result) => {
                drained?;
                scope.options.check()?;
                result
            }
        }
    })
}

#[cfg(test)]
#[path = "local_scope_test.rs"]
mod local_scope_test;
