//! Completion waits and explicit quiescent-scope recovery policy.

use std::time::Instant;

use super::Shared;
use crate::{Error, Result, SuspensionReason, TaskFailure, TaskId, TaskStatus, signal::lock};

impl Shared {
    pub(crate) fn wait(&self, scope: u64, target: Option<TaskId>) -> Result<()> {
        self.wait_until(scope, target, None).map(|_| ())
    }

    pub(crate) fn wait_until(
        &self,
        scope: u64,
        target: Option<TaskId>,
        until: Option<Instant>,
    ) -> Result<bool> {
        let mut quiescent_since = None;
        let mut stalled = None;
        let mut activity = None;
        let mut reported = false;
        loop {
            let observed = self.changed.version();
            let mut state = lock(&self.state);
            let current_activity = state.scopes.get(&scope).map(|scope| scope.activity);
            if activity != current_activity {
                quiescent_since = None;
                reported = false;
                activity = current_activity;
            }
            let mut active = 0;
            let mut quiescent = true;
            let mut target_done = target.is_none();
            for record in state.records.values() {
                let record = lock(record);
                if record.scope != scope {
                    continue;
                }
                if target == Some(record.id) {
                    target_done = record.status.is_terminal();
                }
                if !record.status.is_terminal() {
                    active += 1;
                    quiescent &= matches!(record.status, TaskStatus::Suspended(reason) if !matches!(reason,
                        SuspensionReason::IoRead | SuspensionReason::IoWrite |
                        SuspensionReason::IoAccept | SuspensionReason::IoConnect |
                        SuspensionReason::Blocking | SuspensionReason::Dns | SuspensionReason::FileIo))
                        && record.deadline.is_none();
                }
            }
            if active == 0 || (target.is_some() && target_done && stalled.is_none()) {
                return stalled.map_or(Ok(true), |active| Err(Error::RuntimeStalled { active }));
            }
            if until.is_some_and(|deadline| Instant::now() >= deadline) {
                return Ok(false);
            }
            let scope_state = state.scopes.get(&scope);
            let recovering =
                scope_state.is_some_and(|scope| scope.aborting.is_some()) || !state.accepting;
            let supervised = scope_state.is_some_and(|scope| scope.supervised);
            quiescent &= !self.inboxes.iter().any(|inbox| {
                inbox.hub.pending_tasks().iter().any(|task| {
                    state
                        .records
                        .get(task)
                        .is_some_and(|record| lock(record).scope == scope)
                })
            });
            let deadline =
                if quiescent && !supervised && !recovering && stalled.is_none() && !reported {
                    let since = *quiescent_since.get_or_insert_with(Instant::now);
                    self.config
                        .stall_policy()
                        .timeout()
                        .and_then(|timeout| since.checked_add(timeout))
                } else {
                    quiescent_since = None;
                    None
                };
            if deadline.is_some_and(|deadline| deadline <= Instant::now()) {
                let detected_at = Instant::now();
                state.last_stall = Some(crate::StallSnapshot {
                    policy: self.config.stall_policy(),
                    scope,
                    detected_at,
                    quiescent_for: detected_at.duration_since(quiescent_since.expect("quiescent")),
                    tasks: state
                        .records
                        .values()
                        .filter_map(|record| {
                            let record = lock(record);
                            (record.scope == scope && !record.status.is_terminal())
                                .then(|| record.snapshot())
                        })
                        .collect(),
                });
                reported = true;
                if !self.config.stall_policy().aborts() {
                    drop(state);
                    self.changed.notify();
                    continue;
                }
                stalled = Some(active);
                if let Some(scope) = state.scopes.get_mut(&scope) {
                    scope.aborting = Some(TaskFailure::ScopeStalled);
                }
                drop(state);
                for inbox in &self.inboxes {
                    inbox.abort(scope, TaskFailure::ScopeStalled);
                }
                continue;
            }
            drop(state);
            self.changed
                .wait(observed, deadline.into_iter().chain(until).min());
        }
    }
}

#[cfg(test)]
#[path = "control_wait_test.rs"]
mod control_wait_test;
