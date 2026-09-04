//! Cancellation wait routing for standalone tokens and runtime-owned domains.

use super::{CancellationToken, Node};
use crate::{Error, Result, signal::lock, wait::WaitRegistration};
use std::sync::{
    OnceLock, Weak,
    atomic::{AtomicU64, Ordering},
};
use vthread_stack::ParkToken;

pub(super) struct WaitSlot {
    active: AtomicU64,
    primary: OnceLock<Weak<crate::wait::WaitInner>>,
}

#[derive(Default)]
pub(super) struct FallbackWait {
    pub(super) token: Option<ParkToken>,
    pub(super) registration: Option<WaitRegistration>,
}

impl WaitSlot {
    const FALLBACK: u64 = 1 << 63;

    pub(super) fn new() -> Self {
        Self {
            active: AtomicU64::new(0),
            primary: OnceLock::new(),
        }
    }

    fn active(&self) -> u64 {
        self.active.load(Ordering::SeqCst)
    }

    fn is_primary(&self, wait: &WaitRegistration) -> bool {
        if self.primary.get().is_none() {
            let _ = self.primary.set(wait.state.clone());
        }
        self.primary
            .get()
            .is_some_and(|primary| Weak::ptr_eq(primary, &wait.state))
    }

    fn claim(&self, active: u64) -> Result<()> {
        self.active
            .compare_exchange(0, active, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| ())
            .map_err(|_| concurrent_wait_fault())
    }

    fn release(&self, active: u64) -> bool {
        if self.active.load(Ordering::Acquire) != active {
            return false;
        }
        self.active.store(0, Ordering::Release);
        true
    }
}

impl Node {
    pub(super) fn active_wait(&self) -> Option<(ParkToken, WaitRegistration)> {
        let active = self.wait.active();
        if active == 0 {
            return None;
        }
        if active & WaitSlot::FALLBACK == 0 {
            let state = self.wait.primary.get()?.upgrade()?;
            let registration = WaitRegistration::cached(&state);
            return registration
                .token(active)
                .map(|token| (token, registration));
        }
        let waits = lock(&self.domain.fallback_waits);
        if self.wait.active() != active {
            return None;
        }
        let fallback = waits.get(&self.id())?;
        fallback.token.zip(fallback.registration.as_ref().cloned())
    }

    fn register_wait(&self, token: ParkToken, wait: &WaitRegistration) -> Result<u64> {
        if self.wait.is_primary(wait) {
            let active = token.generation();
            #[cfg(debug_assertions)]
            assert!(active != 0 && active & WaitSlot::FALLBACK == 0);
            self.wait.claim(active)?;
            return Ok(active);
        }

        let mut waits = lock(&self.domain.fallback_waits);
        let epoch = self
            .domain
            .fallback_epoch
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |epoch| {
                epoch
                    .checked_add(1)
                    .filter(|next| *next < WaitSlot::FALLBACK)
            })
            .map_err(|_| fallback_generation_fault())?
            + 1;
        let fallback = waits.entry(self.id()).or_default();
        if fallback
            .registration
            .as_ref()
            .is_none_or(|cached| !cached.same_cell(wait))
        {
            fallback.registration = Some(wait.clone());
        }
        fallback.token = Some(token);
        let active = WaitSlot::FALLBACK | epoch;
        if let Err(error) = self.wait.claim(active) {
            fallback.token = None;
            return Err(error);
        }
        Ok(active)
    }

    fn unregister_wait(&self, active: u64) {
        if active & WaitSlot::FALLBACK == 0 {
            let _ = self.wait.release(active);
            return;
        }
        let mut waits = lock(&self.domain.fallback_waits);
        if self.wait.release(active)
            && let Some(fallback) = waits.get_mut(&self.id())
        {
            fallback.token = None;
        }
    }
}

impl CancellationToken {
    pub(crate) fn register(
        &self,
        token: ParkToken,
        wait: &WaitRegistration,
    ) -> Result<Subscription<'_>> {
        let active = self.0.register_wait(token, wait)?;
        if self.is_cancelled() {
            wait.select_cancelled(token);
        }
        Ok(Subscription {
            node: &self.0,
            active,
        })
    }
}

pub(crate) struct Subscription<'a> {
    node: &'a Node,
    active: u64,
}

impl Drop for Subscription<'_> {
    fn drop(&mut self) {
        self.node.unregister_wait(self.active);
    }
}

fn concurrent_wait_fault() -> Error {
    Error::fault(
        crate::error::FaultComponent::Scheduler,
        "task registered concurrent cancellation waits",
    )
}

fn fallback_generation_fault() -> Error {
    Error::fault(
        crate::error::FaultComponent::Scheduler,
        "cancellation subscription generation exhausted",
    )
}

#[cfg(test)]
#[path = "cancellation_subscription_test.rs"]
mod cancellation_subscription_test;
