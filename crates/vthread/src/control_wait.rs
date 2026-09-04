//! Completion waits and explicit quiescent-scope recovery policy.

use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

use super::Shared;
use crate::task::SharedTaskRecord;
use crate::{Error, Result, SuspensionReason, TaskFailure, TaskStatus, signal::lock};

struct TargetWaiter<'a> {
    count: &'a AtomicUsize,
}

impl<'a> TargetWaiter<'a> {
    fn new(count: &'a AtomicUsize) -> Self {
        count.fetch_add(1, Ordering::SeqCst);
        Self { count }
    }
}

impl Drop for TargetWaiter<'_> {
    fn drop(&mut self) {
        self.count.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Shared {
    pub(crate) fn wait(&self, scope: u64, target: Option<&SharedTaskRecord>) -> Result<()> {
        self.wait_until(scope, target, None).map(|_| ())
    }

    pub(crate) fn wait_until(
        &self,
        scope: u64,
        target: Option<&SharedTaskRecord>,
        until: Option<Instant>,
    ) -> Result<bool> {
        // Register before observing either the signal epoch or target state. A
        // racing completion must then either notify us or be visible below.
        let _target_waiter = target.map(|_| TargetWaiter::new(&self.target_waiters));
        let target_id = target.map(|record| record.lock().id);
        let mut quiescent_since = None;
        let mut stalled = None;
        let mut activity = None;
        let mut reported = false;
        loop {
            let observed = self.changed.version();
            let mut state = lock(&self.state);
            let current_activity = state
                .scopes
                .get(&scope)
                .map(|scope| scope.progress.activity());
            if activity != current_activity {
                quiescent_since = None;
                reported = false;
                activity = current_activity;
            }
            let active = state
                .scopes
                .get(&scope)
                .map_or(0, |scope| scope.progress.active());
            let mut quiescent = true;
            let mut target_done = target.is_none();
            if self.config.stall_policy().timeout().is_some() {
                for entry in state
                    .scopes
                    .get(&scope)
                    .into_iter()
                    .flat_map(|scope| &scope.records)
                {
                    let record = entry.record.lock();
                    if target_id == Some(entry.id) {
                        target_done = entry.record.completion().done();
                    }
                    if !record.status.is_terminal() {
                        quiescent &= matches!(record.status, TaskStatus::Suspended(reason) if !matches!(reason,
                            SuspensionReason::IoRead | SuspensionReason::IoWrite |
                            SuspensionReason::IoAccept | SuspensionReason::IoConnect |
                            SuspensionReason::Blocking | SuspensionReason::Dns | SuspensionReason::FileIo))
                            && record.deadline.is_none();
                    }
                }
            } else if let Some(target) = target {
                target_done = target.completion().done();
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
                    state.scopes.get(&scope).is_some_and(|scope| {
                        scope
                            .records
                            .binary_search_by_key(task, |entry| entry.id)
                            .is_ok()
                    })
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
                let mounted = self
                    .carrier_progress
                    .iter()
                    .map(crate::task_progress::CarrierProgress::mounted)
                    .collect::<Vec<_>>();
                state.last_stall = Some(std::sync::Arc::new(crate::StallSnapshot {
                    policy: self.config.stall_policy(),
                    scope,
                    detected_at,
                    quiescent_for: detected_at.duration_since(quiescent_since.expect("quiescent")),
                    tasks: state
                        .scopes
                        .get(&scope)
                        .into_iter()
                        .flat_map(|scope| &scope.records)
                        .filter_map(|record| {
                            let include = {
                                let task = record.record.lock();
                                !task.status.is_terminal()
                            };
                            include.then(|| record.record.snapshot(&mounted))
                        })
                        .collect(),
                }));
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
                self.abort_requested.store(true, Ordering::Release);
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
