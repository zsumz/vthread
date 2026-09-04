//! Wait winner validation, fast retirement, and abandoned-generation cleanup.

use vthread_stack::ParkToken;

use super::{ResourceSelection, WaitCell, WakeCause, wait_state};
use crate::{Error, Result};

impl WaitCell {
    pub(crate) fn finish(&self, token: ParkToken) -> Result<WakeCause> {
        if token.wait() != self.state.id {
            return Err(resumed_generation_fault());
        }
        self.finish_slow(token)
    }

    pub(crate) fn finish_plain_ready(&self, token: ParkToken) -> Result<WakeCause> {
        self.finish_ready(token, None)
    }

    pub(crate) fn finish_permit_ready(&self, token: ParkToken) -> Result<WakeCause> {
        self.finish_ready(token, Some(ResourceSelection::Permit))
    }

    #[inline]
    fn finish_ready(
        &self,
        token: ParkToken,
        resource: Option<ResourceSelection>,
    ) -> Result<WakeCause> {
        if token.wait() != self.state.id {
            return Err(resumed_generation_fault());
        }
        let selected = wait_state::WaitWord::initial()
            .with_generation(token.generation())
            .with_resource(resource)
            .with_phase(wait_state::Phase::SelectedReady);
        if self
            .state
            .compare_exchange(selected, selected.retire())
            .is_ok()
        {
            #[cfg(feature = "runtime-evidence")]
            self.record_resumed(selected, token, WakeCause::Ready);
            return Ok(WakeCause::Ready);
        }
        self.finish_slow(token)
    }

    fn finish_slow(&self, token: ParkToken) -> Result<WakeCause> {
        loop {
            let word = self.state.load();
            if word.generation() != token.generation() {
                return Err(resumed_generation_fault());
            }
            if word.is_claimed() || word.phase() == wait_state::Phase::Binding {
                std::hint::spin_loop();
                continue;
            }
            let Some(cause) = word.selected_cause() else {
                return Err(Error::fault(
                    crate::error::FaultComponent::Scheduler,
                    "resumed parker has no selected wake",
                ));
            };
            if self.state.compare_exchange(word, word.retire()).is_ok() {
                #[cfg(feature = "runtime-evidence")]
                self.record_resumed(word, token, cause);
                return Ok(cause);
            }
        }
    }

    #[cfg(feature = "runtime-evidence")]
    fn record_resumed(&self, word: wait_state::WaitWord, token: ParkToken, cause: WakeCause) {
        let (task, evidence) = self
            .state
            .with_target(word, |task, _, hub| (task, hub.evidence()));
        if let Some(evidence) = evidence {
            evidence.record(crate::diagnostics::evidence::RuntimeEventKind::Resumed {
                task,
                wait: crate::diagnostics::evidence::WaitKey::from_token(token),
                cause: cause.evidence(),
            });
        }
    }

    pub(crate) fn rollback(&self, token: ParkToken) {
        let Some(hub) = self.state.retire(token) else {
            return;
        };
        crate::context::unregister_local_wake(&hub, token);
        hub.discard_notice(token);
    }

    pub(crate) fn abandon(&self, token: ParkToken) {
        if let Some(hub) = self.state.retire(token) {
            hub.discard_notice(token);
        }
    }
}

fn resumed_generation_fault() -> Error {
    Error::fault(
        crate::error::FaultComponent::Scheduler,
        "resumed parker generation changed",
    )
}

#[cfg(test)]
#[path = "wait_finish_test.rs"]
mod wait_finish_test;
