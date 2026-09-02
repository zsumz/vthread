//! Runtime-wide, nonblocking evidence publication.

impl super::Shared {
    pub(crate) fn record(&self, kind: crate::diagnostics::evidence::RuntimeEventKind) {
        if let Some(evidence) = &self.evidence {
            evidence.record(self.id, kind);
        }
    }

    pub(crate) fn record_task_accepted(&self, record: &crate::task::SharedTaskRecord) {
        let record = record.lock();
        self.record(
            crate::diagnostics::evidence::RuntimeEventKind::TaskAccepted {
                task: record.id,
                scope: crate::diagnostics::ScopeId::new(record.scope),
                parent: record.parent,
                carrier: record.carrier,
            },
        );
    }

    pub(crate) fn record_terminal(
        &self,
        task: crate::TaskId,
        status: crate::TaskStatus,
        failure: Option<crate::TaskFailure>,
    ) {
        use crate::diagnostics::evidence::{RuntimeEventKind, TaskOutcome};
        let outcome = match status {
            crate::TaskStatus::Completed => TaskOutcome::Completed,
            crate::TaskStatus::Panicked => TaskOutcome::Panicked,
            crate::TaskStatus::Aborted => {
                TaskOutcome::Aborted(failure.expect("aborted task has failure"))
            }
            _ => return,
        };
        self.record(RuntimeEventKind::TaskTerminated { task, outcome });
    }

    pub(crate) fn record_admission_rejected(
        &self,
        resource: crate::error::CapacityResource,
        limit: usize,
    ) {
        self.record(
            crate::diagnostics::evidence::RuntimeEventKind::AdmissionRejected { resource, limit },
        );
    }
}

#[cfg(test)]
#[path = "control_evidence_test.rs"]
mod control_evidence_test;
