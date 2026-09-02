//! Evidence emitted at exact wait-winner linearization points.

use super::{ActiveWait, WaitRegistration, WakeCause};
use vthread_stack::ParkToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SelectionRejection {
    NoWait,
    NoActive,
    Retired,
    Selected,
}

impl WaitRegistration {
    pub(super) fn record_selected(&self, token: ParkToken, cause: WakeCause) {
        #[cfg(feature = "runtime-evidence")]
        if let (Some(evidence), Some(task)) = (&self.evidence, self.task) {
            let wait = crate::diagnostics::evidence::WaitKey::from_token(token);
            let cause = cause.evidence();
            let origin = wake_origin();
            evidence.record(
                crate::diagnostics::evidence::RuntimeEventKind::WakeOffered {
                    task,
                    wait,
                    cause,
                    origin,
                },
            );
            evidence.record(
                crate::diagnostics::evidence::RuntimeEventKind::WakeSelected {
                    task,
                    wait,
                    cause,
                    origin,
                },
            );
        }
        #[cfg(not(feature = "runtime-evidence"))]
        let _ = (token, cause);
    }

    pub(super) fn record_rejected(
        &self,
        token: ParkToken,
        cause: WakeCause,
        rejection: SelectionRejection,
    ) {
        #[cfg(feature = "runtime-evidence")]
        {
            let reason = match rejection {
                SelectionRejection::NoWait => crate::diagnostics::evidence::WakeRejection::NoWait,
                SelectionRejection::NoActive => {
                    crate::diagnostics::evidence::WakeRejection::NoActiveWait
                }
                SelectionRejection::Retired => {
                    crate::diagnostics::evidence::WakeRejection::RetiredGeneration
                }
                SelectionRejection::Selected => {
                    crate::diagnostics::evidence::WakeRejection::AlreadySelected
                }
            };
            if let (Some(evidence), Some(task)) = (&self.evidence, self.task) {
                let wait = crate::diagnostics::evidence::WaitKey::from_token(token);
                let cause = cause.evidence();
                let origin = wake_origin();
                evidence.record(
                    crate::diagnostics::evidence::RuntimeEventKind::WakeOffered {
                        task,
                        wait,
                        cause,
                        origin,
                    },
                );
                evidence.record(
                    crate::diagnostics::evidence::RuntimeEventKind::WakeRejected {
                        task,
                        wait,
                        cause,
                        origin,
                        reason,
                    },
                );
            }
        }
        #[cfg(not(feature = "runtime-evidence"))]
        let _ = (token, cause, rejection);
    }
}

pub(super) fn record_current(active: &ActiveWait, cause: WakeCause) {
    #[cfg(feature = "runtime-evidence")]
    if let Some(evidence) = &active.evidence {
        let wait = crate::diagnostics::evidence::WaitKey::from_token(active.token);
        let cause = cause.evidence();
        let origin = wake_origin();
        evidence.record(
            crate::diagnostics::evidence::RuntimeEventKind::WakeOffered {
                task: active.task,
                wait,
                cause,
                origin,
            },
        );
        evidence.record(
            crate::diagnostics::evidence::RuntimeEventKind::WakeSelected {
                task: active.task,
                wait,
                cause,
                origin,
            },
        );
    }
    #[cfg(not(feature = "runtime-evidence"))]
    let _ = (active, cause);
}

#[cfg(feature = "runtime-evidence")]
fn wake_origin() -> crate::diagnostics::evidence::WakeOrigin {
    crate::worker_context::current_carrier().map_or(
        crate::diagnostics::evidence::WakeOrigin::External,
        crate::diagnostics::evidence::WakeOrigin::Carrier,
    )
}

#[cfg(test)]
#[path = "wait_evidence_test.rs"]
mod wait_evidence_test;
