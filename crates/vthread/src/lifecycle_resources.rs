//! Independent completion proofs retained by the process lifecycle table.

use crate::{control::Shared, join_slot::JoinSlots};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};

#[derive(Default)]
pub(crate) struct CoordinatorResources {
    pub(crate) workers: Arc<JoinSlots>,
    pub(crate) readiness: OnceLock<Arc<JoinSlots>>,
    pub(crate) native: OnceLock<Arc<JoinSlots>>,
    pub(crate) returned: AtomicBool,
}

impl CoordinatorResources {
    pub(crate) fn carriers_reclaimed(&self, shared: &Shared) -> bool {
        self.workers.joined() && shared.inboxes.iter().all(|inbox| inbox.cleanup_complete())
    }

    pub(crate) fn drained(&self, shared: &Shared, coordinator_joined: bool) -> bool {
        coordinator_joined
            && self.returned.load(Ordering::Acquire)
            && self.carriers_reclaimed(shared)
            && self.readiness.get().is_none_or(|slots| slots.joined())
            && self.native.get().is_none_or(|slots| slots.joined())
            && shared.services.get().is_none_or(|services| {
                services.reactor.cleanup_complete() && services.blocking.cleanup_complete()
            })
    }
}

#[cfg(test)]
#[path = "lifecycle_resources_test.rs"]
mod lifecycle_resources_test;
