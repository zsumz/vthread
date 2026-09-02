//! Scope ownership, inherited policy, and explicit supervision records.

use super::Shared;
use crate::{Error, Result, ScopeOptions, TaskFailure, options::TaskOptions, signal::lock};
use std::sync::atomic::Ordering;

pub(super) struct ScopeState {
    pub(super) admitting: bool,
    pub(super) activity: u64,
    pub(super) options: TaskOptions,
    pub(super) supervised: bool,
    pub(super) aborting: Option<TaskFailure>,
    pub(super) completed: u64,
    pub(super) panicked: u64,
    pub(super) aborted: u64,
}

impl Shared {
    pub(crate) fn abort_reason(&self, scope: u64) -> Option<TaskFailure> {
        if !self.abort_requested.load(Ordering::Acquire) {
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
                activity: 0,
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
        let mut state = lock(&self.state);
        state
            .records
            .retain(|_, record| record.lock().scope != scope);
        state.scopes.remove(&scope);
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
        for inbox in &self.inboxes {
            inbox.clear_abort(scope);
        }
    }

    pub(crate) fn unobserved(&self, scope: u64) -> crate::ScopeFailure {
        let mut failures = crate::ScopeFailure::default();
        for record in lock(&self.state).records.values() {
            let record = record.lock();
            if record.scope != scope || record.outcome_observed {
                continue;
            }
            if let Some(reason) = record.failure {
                failures.child_failed(Error::TaskAborted {
                    task: record.id,
                    reason,
                });
            } else if let Some(panic) = &record.panic {
                failures.child_failed(Error::task_panicked(
                    record.id,
                    record.name.to_string(),
                    panic.clone(),
                ));
            }
        }
        failures
    }
}

#[cfg(test)]
#[path = "control_scope_test.rs"]
mod control_scope_test;
