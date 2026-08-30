//! Completion waits and explicit quiescent-scope recovery policy.

use std::time::Instant;

use super::Shared;
use crate::{Error, Result, SuspensionReason, TaskFailure, TaskId, TaskStatus, signal::lock};

impl Shared {
    pub(crate) fn wait(&self, scope: u64, target: Option<TaskId>) -> Result<()> {
        let mut quiescent_since = None;
        let mut stalled = None;
        let mut activity = None;
        loop {
            let observed = self.changed.version();
            let mut state = lock(&self.state);
            if activity != Some(state.activity) {
                quiescent_since = None;
                activity = Some(state.activity);
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
                    quiescent &= record.status == TaskStatus::Suspended(SuspensionReason::Park)
                        && record.deadline.is_none();
                }
            }
            if active == 0 || (target.is_some() && target_done && stalled.is_none()) {
                return stalled.map_or(Ok(()), |active| Err(Error::RuntimeStalled { active }));
            }
            let recovering = state.aborting.is_some() || !state.accepting;
            quiescent &= self.inboxes.iter().all(|inbox| inbox.hub.pending() == 0);
            let deadline = if quiescent && !recovering && stalled.is_none() {
                let since = *quiescent_since.get_or_insert_with(Instant::now);
                self.config
                    .stall_timeout()
                    .and_then(|timeout| since.checked_add(timeout))
            } else {
                quiescent_since = None;
                None
            };
            if deadline.is_some_and(|deadline| deadline <= Instant::now()) {
                stalled = Some(active);
                state.aborting = Some(TaskFailure::ScopeStalled);
                drop(state);
                for inbox in &self.inboxes {
                    inbox.abort(scope, TaskFailure::ScopeStalled);
                }
                continue;
            }
            drop(state);
            self.changed.wait(observed, deadline);
        }
    }
}

#[cfg(test)]
#[path = "control_wait_test.rs"]
mod control_wait_test;
