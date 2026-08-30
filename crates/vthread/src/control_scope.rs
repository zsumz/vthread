//! Scope ownership, inherited policy, and explicit supervision records.

use super::Shared;
use crate::{Error, Result, ScopeOptions, TaskFailure, options::TaskOptions, signal::lock};

pub(super) struct ScopeState {
    pub(super) options: TaskOptions,
    pub(super) supervised: bool,
    pub(super) aborting: Option<TaskFailure>,
    pub(super) completed: u64,
    pub(super) panicked: u64,
    pub(super) aborted: u64,
}

impl Shared {
    pub(crate) fn abort_reason(&self, scope: u64) -> Option<TaskFailure> {
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
            return Err(Error::NestedScope);
        }
        if state.scopes.len() >= self.config.max_vthreads() {
            return Err(Error::AtCapacity {
                limit: self.config.max_vthreads(),
            });
        }
        let id = state.next_scope;
        state.next_scope = id
            .checked_add(1)
            .ok_or(Error::Invariant("scope id space exhausted"))?;
        if !supervised {
            state.active_scope = Some(id);
        }
        state.scopes.insert(
            id,
            ScopeState {
                options: TaskOptions::root(options, self.config.max_vthreads()),
                supervised,
                aborting: None,
                completed: 0,
                panicked: 0,
                aborted: 0,
            },
        );
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
            scope.options.cancellation.clone()
        };
        for inbox in &self.inboxes {
            inbox.abort(scope, reason);
        }
        cancellation.cancel();
        self.changed.notify();
    }

    pub(crate) fn finish_scope(&self, scope: u64) {
        let mut state = lock(&self.state);
        state
            .records
            .retain(|_, record| lock(record).scope != scope);
        state.scopes.remove(&scope);
        if state.active_scope == Some(scope) {
            state.active_scope = None;
        }
        for inbox in &self.inboxes {
            inbox.clear_abort(scope);
        }
    }

    pub(crate) fn unobserved(&self, scope: u64) -> Result<()> {
        for record in lock(&self.state).records.values() {
            let record = lock(record);
            if record.scope != scope || record.outcome_observed {
                continue;
            }
            if let Some(reason) = record.failure {
                return Err(Error::TaskAborted {
                    task: record.id,
                    reason,
                });
            }
            if let Some(panic) = &record.panic {
                return Err(Error::task_panicked(
                    record.id,
                    record.name.to_string(),
                    panic.clone(),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "control_scope_test.rs"]
mod control_scope_test;
