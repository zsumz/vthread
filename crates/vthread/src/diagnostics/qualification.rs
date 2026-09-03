//! Explicit generation-bound wake handles for runtime qualification.
//!
//! These handles exercise the same wait selection path as readiness and cancellation.
//! Retain a generation handle, let that generation retire, then offer it again to prove
//! that a stale wake is rejected. This module is available only with `qualification`.

use crate::{Result, parking::ParkOutcome, signal::lock, wait::WaitRegistration};
use std::sync::{Arc, Mutex};
use vthread_stack::ParkToken;

/// Whether a generation-bound wake offer selected its target.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
#[non_exhaustive]
pub enum ProbeWakeResult {
    /// The generation had no prior winner and accepted this offer.
    Selected,
    /// The generation was retired, absent, or already selected.
    Rejected,
}

/// A single-consumer parker that publishes exact generation-bound wake handles.
pub struct ProbeParker {
    parker: crate::parking::Parker,
    latest: Arc<Mutex<Option<GenerationWake>>>,
}

impl ProbeParker {
    /// Parks the current virtual thread and publishes this generation to its source.
    pub fn park(&self) -> Result<ParkOutcome> {
        let latest = Arc::clone(&self.latest);
        self.parker.park_registered(move |token, registration| {
            *lock(&latest) = Some(GenerationWake {
                token,
                registration: registration.clone(),
            });
            Ok(())
        })
    }
}

/// Outside-task access to the most recently published probe generation.
#[derive(::core::clone::Clone)]
pub struct GenerationSource {
    latest: Arc<Mutex<Option<GenerationWake>>>,
}

impl GenerationSource {
    /// Takes the latest published generation, or `None` before publication.
    pub fn take(&self) -> Option<GenerationWake> {
        lock(&self.latest).take()
    }
}

/// A wake source permanently bound to one exact wait generation.
#[derive(::core::clone::Clone)]
pub struct GenerationWake {
    token: ParkToken,
    registration: WaitRegistration,
}

impl GenerationWake {
    /// Returns the exact reusable-wait identity and generation captured by this handle.
    pub fn wait_key(&self) -> crate::diagnostics::evidence::WaitKey {
        crate::diagnostics::evidence::WaitKey::from_token(self.token)
    }

    /// Offers readiness to the captured generation through normal winner selection.
    pub fn offer_ready(&self) -> ProbeWakeResult {
        if self.registration.select_ready(self.token) {
            ProbeWakeResult::Selected
        } else {
            ProbeWakeResult::Rejected
        }
    }
}

/// Creates a qualification parker and its generation source.
pub fn probe_pair() -> (ProbeParker, GenerationSource) {
    let (parker, _) = crate::parking::park_pair();
    let latest = Arc::new(Mutex::new(None));
    (
        ProbeParker {
            parker,
            latest: Arc::clone(&latest),
        },
        GenerationSource { latest },
    )
}

#[cfg(test)]
#[path = "qualification_test.rs"]
mod qualification_test;
