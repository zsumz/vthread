//! Wait-generation activation with a direct path for stable task ownership.

use std::{sync::Arc, time::Instant};

use vthread_stack::{ParkRequest, ParkToken};

use crate::{Error, Result, TaskId, task_slab::TaskKey};

use super::{
    WaitBegin, WaitCell, WaitHub, WaitRegistration, WakeCause,
    wait_state::{MAX_GENERATION, Phase, WaitWord},
};

impl WaitCell {
    pub(crate) fn begin(
        &self,
        task: TaskId,
        route: TaskKey,
        hub: &Arc<WaitHub>,
        deadline: Option<Instant>,
    ) -> Result<WaitBegin> {
        Ok(self
            .begin_resident(task, route, hub, deadline)?
            .map_registration(|()| WaitRegistration::cached(&self.state)))
    }

    pub(crate) fn begin_resident(
        &self,
        task: TaskId,
        route: TaskKey,
        hub: &Arc<WaitHub>,
        deadline: Option<Instant>,
    ) -> Result<WaitBegin<()>> {
        let mut word = self.state.load();
        loop {
            if word.phase() != Phase::Idle {
                return Err(Error::ParkerBusy);
            }
            if word.is_closed() {
                return Ok(WaitBegin::Immediate(WakeCause::Closed));
            }
            if word.has_permit() {
                match self.state.compare_exchange(word, word.with_permit(false)) {
                    Ok(()) => return Ok(WaitBegin::Immediate(WakeCause::Ready)),
                    Err(observed) => {
                        word = observed;
                        continue;
                    }
                }
            }
            if deadline.is_some_and(|deadline| deadline <= Instant::now()) {
                return Ok(WaitBegin::Immediate(WakeCause::TimedOut));
            }
            let generation = next_generation(word)?;
            if let Some(fallback) = self.state.cached_target(task, route, hub) {
                let active = word
                    .with_generation(generation)
                    .with_fallback_hub(fallback)
                    .with_phase(Phase::Active);
                match self.state.compare_exchange(word, active) {
                    Ok(()) => return Ok(self.parked(task, hub, deadline, generation)),
                    Err(observed) => {
                        word = observed;
                        continue;
                    }
                }
            }
            let binding = word.with_generation(generation).with_phase(Phase::Binding);
            match self.state.compare_exchange(word, binding) {
                Ok(()) => {
                    let fallback = self.state.bind_target(task, route, hub);
                    self.state.store(
                        binding
                            .with_fallback_hub(fallback)
                            .with_phase(Phase::Active),
                    );
                    return Ok(self.parked(task, hub, deadline, generation));
                }
                Err(observed) => word = observed,
            }
        }
    }

    fn parked(
        &self,
        task: TaskId,
        hub: &Arc<WaitHub>,
        deadline: Option<Instant>,
        generation: u64,
    ) -> WaitBegin<()> {
        let token = ParkToken::new(self.state.id, generation);
        #[cfg(not(feature = "runtime-evidence"))]
        let _ = (task, hub);
        #[cfg(feature = "runtime-evidence")]
        hub.record(
            crate::diagnostics::evidence::RuntimeEventKind::WaitPublished {
                task,
                wait: crate::diagnostics::evidence::WaitKey::from_token(token),
                has_deadline: deadline.is_some(),
            },
        );
        WaitBegin::Park {
            request: ParkRequest::new(token, deadline),
            registration: (),
        }
    }
}

fn next_generation(word: WaitWord) -> Result<u64> {
    word.generation()
        .checked_add(1)
        .filter(|generation| *generation <= MAX_GENERATION)
        .ok_or_else(|| {
            Error::fault(
                crate::error::FaultComponent::Scheduler,
                "wait generation space exhausted",
            )
        })
}

#[cfg(test)]
#[path = "wait_begin_test.rs"]
mod wait_begin_test;
