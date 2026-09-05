//! Resource claims separated from wake routing, with write-exclusive publication.

use super::{
    ResourceSelection, WaitCell, WakeCause,
    wait_select::enqueue_selected,
    wait_state::{Phase, WaitWord},
};

/// Owns an already selected resource until its wake has been published.
/// Moving the queue's existing WaitCell here does not clone its Arc.
pub(crate) struct ResourcePublication {
    wait: WaitCell,
    claimed: Option<WaitWord>,
}

impl ResourcePublication {
    pub(crate) fn publish(self) {
        drop(self);
    }
}

impl Drop for ResourcePublication {
    fn drop(&mut self) {
        // Unwinding between queue unlock and explicit publication must not strand
        // a claimed generation or the resource that now belongs to its recipient.
        if let Some(claimed) = self.claimed.take() {
            enqueue_selected(&self.wait.state, claimed, WakeCause::Ready, None, true);
        }
    }
}

impl WaitCell {
    pub(crate) fn offer_resource(&self, selection: ResourceSelection) -> bool {
        let Some(claimed) = self.claim_resource(selection) else {
            return false;
        };
        if let Some(claimed) = claimed {
            enqueue_selected(&self.state, claimed, WakeCause::Ready, None, true);
        }
        true
    }

    /// Selects under the primitive's queue lock; the returned guard routes outside it.
    pub(crate) fn reserve_resource(
        self,
        selection: ResourceSelection,
    ) -> Option<ResourcePublication> {
        let claimed = self.claim_resource(selection)?;
        Some(ResourcePublication {
            wait: self,
            claimed,
        })
    }

    // None rejects the offer; Some(None) stores a resource before a park begins.
    fn claim_resource(&self, selection: ResourceSelection) -> Option<Option<WaitWord>> {
        let mut word = self.state.load();
        loop {
            if word.phase() == Phase::Binding {
                std::hint::spin_loop();
                word = self.state.load();
                continue;
            }
            let next = word.resource_offer(selection)?;
            let active = word.phase() == Phase::Active;
            match self.state.compare_exchange(word, next) {
                Ok(()) => return Some(active.then_some(next)),
                Err(observed) => word = observed,
            }
        }
    }

    pub(crate) fn take_resource(&self) -> Option<ResourceSelection> {
        loop {
            if let Some(resource) = self.try_take_resource() {
                return resource;
            }
            std::hint::spin_loop();
        }
    }

    fn try_take_resource(&self) -> Option<Option<ResourceSelection>> {
        let word = self.state.load();
        if word.resource().is_none() {
            return Some(None);
        }
        // Binding and Claim own the entire word until their release store.
        // Clearing a resource here would be overwritten by that publication.
        let (next, resource) = word.resource_take()?;
        self.state
            .compare_exchange(word, next)
            .ok()
            .map(|()| Some(resource))
    }
}

#[cfg(test)]
#[path = "wait_resource_test.rs"]
mod wait_resource_test;
