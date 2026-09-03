//! Scope ownership, inherited policy, and explicit supervision records.

use super::Shared;
use crate::{
    Error, Result, ScopeOptions, TaskFailure, options::TaskOptions, signal::lock,
    task::SharedTaskRecord,
};
use std::sync::{Arc, atomic::Ordering};

pub(super) struct ScopeRecord {
    pub(super) id: crate::TaskId,
    pub(super) record: SharedTaskRecord,
}

pub(super) struct ScopeState {
    pub(super) admitting: bool,
    pub(super) active: usize,
    pub(super) activity: u64,
    // Monotonic task identities keep this admission-ordered vector searchable.
    pub(super) records: Vec<ScopeRecord>,
    pub(super) failed_tasks: Vec<crate::TaskId>,
    pub(super) options: TaskOptions,
    pub(super) supervised: bool,
    pub(super) aborting: Option<TaskFailure>,
    pub(super) completed: u64,
    pub(super) panicked: u64,
    pub(super) aborted: u64,
}

impl Shared {
    pub(crate) fn abort_requested(&self) -> bool {
        self.abort_requested.load(Ordering::Acquire)
    }

    pub(crate) fn abort_reason(&self, scope: u64) -> Option<TaskFailure> {
        if !self.abort_requested() {
            return None;
        }
        let state = lock(&self.state);
        if !state.accepting {
            return Some(TaskFailure::RuntimeStopped);
        }
        state.scopes.get(&scope).and_then(|scope| scope.aborting)
    }

    pub(crate) fn scope_report(&self, scope: u64) -> crate::ShutdownReport {
        let state = lock(&self.state);
        let Some(scope) = state.scopes.get(&scope) else {
            return crate::ShutdownReport::default();
        };
        crate::ShutdownReport {
            failures: lock(&self.failures).clone(),
            completed: scope.completed,
            panicked: scope.panicked,
            aborted: scope.aborted,
            failed_carriers: state
                .carriers
                .iter()
                .filter(|carrier| carrier.status == crate::CarrierStatus::Failed)
                .count(),
        }
    }

    #[cfg(test)]
    pub(crate) fn begin_scope(&self) -> Result<u64> {
        self.begin_owned(ScopeOptions::default(), false)
    }

    pub(crate) fn begin_owned(&self, options: ScopeOptions, supervised: bool) -> Result<u64> {
        let mut state = lock(&self.state);
        if !state.accepting {
            return Err(Error::RuntimeStopped);
        }
        if !supervised && state.active_scope.is_some() {
            return Err(Error::RootScopeActive);
        }
        if state.scopes.len() >= self.config.max_owned_scopes() {
            #[cfg(feature = "runtime-evidence")]
            self.record_admission_rejected(
                crate::error::CapacityResource::Scopes,
                self.config.max_owned_scopes(),
            );
            return Err(Error::Capacity {
                resource: crate::error::CapacityResource::Scopes,
                limit: self.config.max_owned_scopes(),
            });
        }
        let id = state.next_scope;
        state.next_scope = id.checked_add(1).ok_or(Error::fault(
            crate::error::FaultComponent::Lifecycle,
            "scope id space exhausted",
        ))?;
        if !supervised {
            state.active_scope = Some(id);
        }
        state.scopes.insert(
            id,
            ScopeState {
                admitting: true,
                active: 0,
                activity: 0,
                records: Vec::new(),
                failed_tasks: Vec::new(),
                options: TaskOptions {
                    cancellation: self.cancellation.child_token(),
                    deadline: options.deadline,
                },
                supervised,
                aborting: None,
                completed: 0,
                panicked: 0,
                aborted: 0,
            },
        );
        #[cfg(feature = "runtime-evidence")]
        self.record(
            crate::diagnostics::evidence::RuntimeEventKind::ScopeOpened {
                scope: crate::diagnostics::ScopeId::new(id),
                parent: None,
                supervised,
            },
        );
        drop(state);
        Ok(id)
    }

    pub(crate) fn scope_options(&self, scope: u64) -> Result<TaskOptions> {
        lock(&self.state)
            .scopes
            .get(&scope)
            .map(|scope| scope.options.clone())
            .ok_or(Error::RuntimeStopped)
    }

    pub(crate) fn abort_scope(&self, scope: u64, reason: TaskFailure) {
        let cancellation = {
            let mut state = lock(&self.state);
            let Some(scope) = state.scopes.get_mut(&scope) else {
                return;
            };
            scope.aborting = Some(reason);
            scope.admitting = false;
            self.abort_requested.store(true, Ordering::Release);
            scope.options.cancellation.clone()
        };
        for inbox in &self.inboxes {
            inbox.abort(scope, reason);
        }
        cancellation.cancel();
        self.changed.notify();
    }

    pub(crate) fn close_scope(&self, scope: u64) {
        // The same lock reserves tasks, so every admission is either counted before
        // this transition or rejected. Local children remain owned by active parents.
        if let Some(scope) = lock(&self.state).scopes.get_mut(&scope) {
            scope.admitting = false;
        }
    }

    pub(crate) fn finish_scope(&self, scope: u64) {
        #[cfg(feature = "lifecycle-profiling")]
        let retirement_started = std::time::Instant::now();
        #[cfg(feature = "lifecycle-profiling")]
        let mut retired = 0;
        let mut state = lock(&self.state);
        if let Some(scope_state) = state.scopes.remove(&scope) {
            state.record_count -= scope_state.records.len();
            let mut cache = std::mem::take(&mut state.record_cache);
            for ScopeRecord { mut record, .. } in scope_state.records {
                #[cfg(feature = "lifecycle-profiling")]
                {
                    retired += 1;
                }
                if cache.len() < self.config.stack_cache_capacity()
                    && let Some(cell) = Arc::get_mut(&mut record)
                {
                    drop(cell.recycle());
                    cache.push(record);
                }
            }
            state.record_cache = cache;
        }
        if state.active_scope == Some(scope) {
            state.active_scope = None;
        }
        let abort_requested =
            !state.accepting || state.scopes.values().any(|scope| scope.aborting.is_some());
        self.abort_requested
            .store(abort_requested, Ordering::Release);
        #[cfg(feature = "runtime-evidence")]
        self.record(
            crate::diagnostics::evidence::RuntimeEventKind::ScopeClosed {
                scope: crate::diagnostics::ScopeId::new(scope),
            },
        );
        drop(state);
        #[cfg(feature = "lifecycle-profiling")]
        self.lifecycle_probe
            .record_retirement(retirement_started.elapsed(), retired);
        for inbox in &self.inboxes {
            inbox.clear_abort(scope);
        }
    }

    pub(crate) fn unobserved(&self, scope: u64) -> crate::ScopeFailure {
        let mut failures = crate::ScopeFailure::default();
        let state = lock(&self.state);
        let Some(scope_state) = state.scopes.get(&scope) else {
            return failures;
        };
        let mut children = Vec::with_capacity(scope_state.failed_tasks.len());
        for task in &scope_state.failed_tasks {
            let Ok(index) = scope_state
                .records
                .binary_search_by_key(task, |record| record.id)
            else {
                continue;
            };
            let record = &scope_state.records[index].record;
            let record = record.lock();
            if record.outcome_observed {
                continue;
            }
            let error = if let Some(reason) = record.failure {
                Some(Error::TaskAborted {
                    task: record.id,
                    reason,
                })
            } else {
                record.panic.as_ref().map(|panic| {
                    Error::task_panicked(record.id, record.name.to_string(), panic.clone())
                })
            };
            children.extend(error.map(|error| (record.id, error)));
        }
        drop(state);
        children.sort_unstable_by_key(|(task, _)| *task);
        for (_, error) in children {
            failures.child_failed(error);
        }
        failures
    }
}

#[cfg(test)]
#[path = "control_scope_test.rs"]
mod control_scope_test;
