//! Shared production publication, completion-interest and retirement protocol.

use super::{HubHandle, ParkToken, Phase, WaitInner, WaitWord};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Publication {
    Published,
    InFlight,
    Stale,
}

impl WaitInner {
    #[inline]
    pub(super) fn publish_claim(&self, claimed: WaitWord) {
        // Only owner completion interest may change Claim. All old route writes
        // precede this release; the optional completion signal owns its hub.
        if let Err(observed) = self.compare_exchange(claimed, claimed.publish_claim()) {
            self.publish_observed_claim(claimed, observed);
        }
    }

    pub(super) fn publication(&self, token: ParkToken) -> Publication {
        if token.wait() != self.id {
            return Publication::Stale;
        }
        loop {
            let word = self.load();
            if word.generation() != token.generation() {
                return Publication::Stale;
            }
            if word.selected_cause().is_some() {
                return Publication::Published;
            }
            if !word.is_claimed() {
                return Publication::Stale;
            }
            if self.request_completion(word, token) {
                return Publication::InFlight;
            }
        }
    }

    fn request_completion(&self, word: WaitWord, token: ParkToken) -> bool {
        // Activation consumes stored permits before Active. Only the owner may
        // set this otherwise absent bit while claimed; selectors/resource takers
        // reject Claim and permit/close mutation waits for Selected.
        if word.has_permit() || self.compare_exchange(word, word.with_permit(true)).is_ok() {
            #[cfg(test)]
            self.observe_deferred(token);
            #[cfg(not(test))]
            let _ = token;
            true
        } else {
            false
        }
    }

    #[cold]
    pub(super) fn publish_observed_claim(&self, claimed: WaitWord, observed: WaitWord) {
        assert_eq!(
            observed.with_permit(false),
            claimed,
            "only owner interest may change a claim"
        );
        assert!(
            observed.has_permit(),
            "publication CAS failed without owner interest"
        );
        // Retain the exact hub BEFORE releasing the claim. After this CAS the
        // owner may retire/rebind/reuse the wait, so do not read its target again.
        let hub = self.clone_hub(observed);
        assert!(
            self.compare_exchange(observed, observed.publish_claim().with_permit(false))
                .is_ok()
        );
        #[cfg(test)]
        self.observe_completion(ParkToken::new(self.id, claimed.generation()));
        hub.publication_complete();
    }

    pub(super) fn try_retire(&self, token: ParkToken) -> Result<Option<HubHandle>, ()> {
        if token.wait() != self.id {
            return Ok(None);
        }
        loop {
            let word = self.load();
            if word.generation() != token.generation() || word.phase() == Phase::Idle {
                return Ok(None);
            }
            // Owner calls require a token returned by begin(), after Binding
            // finished. A newer Binding has a different generation and is stale.
            assert_ne!(
                word.phase(),
                Phase::Binding,
                "owner retirement preceded binding"
            );
            if word.is_claimed() {
                if self.request_completion(word, token) {
                    #[cfg(test)]
                    self.observe_retirement(token);
                    return Err(());
                }
                continue;
            }
            let hub = self.clone_hub(word);
            // Keep selected resource bits until the actual primitive ticket's
            // cleanup consumes/returns ownership. Retirement is not resource Drop.
            if self.compare_exchange(word, word.retire()).is_ok() {
                return Ok(Some(hub));
            }
        }
    }
}

#[cfg(test)]
#[path = "wait_owner_protocol_test.rs"]
mod wait_owner_protocol_test;
