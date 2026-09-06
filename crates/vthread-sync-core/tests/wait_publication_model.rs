//! Proposed publication/owner-deferral protocol, not a shipped runtime repair.
//!
//! WaitWord is production source. The bounded one-route transport abstracts the
//! existing wake queue; Signal preserves its epoch/waiter/gate ordering. Native
//! queue reversal, target binding, stack destruction and multi-route composition
//! still require their own integration evidence. No production atomic is modeled
//! through std: all mutable shared state below uses Loom primitives.

#![forbid(unsafe_code)]

use loom::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WakeCause {
    Ready,
    TimedOut,
    Cancelled,
    InheritedCancelled,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResourceSelection {
    Permit,
    Broadcast,
}

mod wait {
    pub(super) use super::{ResourceSelection, WakeCause};
}

#[path = "support/publication_composition_test.rs"]
mod publication_composition_test;
#[path = "support/publication_signal_test.rs"]
mod publication_signal;
#[path = "../../vthread/src/wait_state.rs"]
mod wait_state;

use publication_signal::Route;
use wait_state::{Phase, WaitWord};

fn model(f: impl Fn() + Send + Sync + 'static) {
    let mut builder = loom::model::Builder::new();
    builder.max_threads = 3;
    builder.max_branches = 1_000;
    builder.max_permutations = None;
    builder.max_duration = None;
    builder.preemption_bound = None;
    builder.checkpoint_file = None;
    builder.check(f);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Publication {
    Published,
    InFlight,
    Stale,
}

struct Handoff {
    word: AtomicU64,
    route: Route,
    ownership: AtomicUsize,
    recovered: AtomicUsize,
    consumed: AtomicUsize,
}

impl Handoff {
    fn new() -> Self {
        Self {
            word: AtomicU64::new(Self::active(41).raw()),
            route: Route::new(),
            ownership: AtomicUsize::new(0),
            recovered: AtomicUsize::new(0),
            consumed: AtomicUsize::new(0),
        }
    }

    fn active(generation: u64) -> WaitWord {
        WaitWord::initial()
            .with_generation(generation)
            .with_phase(Phase::Active)
    }

    fn load(&self) -> WaitWord {
        WaitWord::from_raw(self.word.load(Ordering::Acquire))
    }

    fn replace(&self, before: WaitWord, after: WaitWord) -> bool {
        self.word
            .compare_exchange(
                before.raw(),
                after.raw(),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    fn claim(&self, cause: WakeCause, resource: bool) -> Option<WaitWord> {
        let before = self.load();
        if before.phase() != Phase::Active {
            return None;
        }
        assert!(
            !before.has_permit(),
            "activation consumed every stored permit"
        );
        let claimed = before
            .claimed(cause)
            .with_closed(cause == WakeCause::Closed)
            .with_resource(resource.then_some(ResourceSelection::Permit));
        if !self.replace(before, claimed) {
            return None;
        }
        if resource {
            assert_eq!(self.ownership.swap(1, Ordering::Release), 0);
        }
        Some(claimed)
    }

    fn publish(&self, claimed: WaitWord, complete_notice: bool) {
        let mut before = claimed;
        loop {
            // During a claim, the otherwise absent permit bit can represent
            // owner interest. It is never a permit for the next generation.
            let selected = before.publish_claim().with_permit(false);
            if self.replace(before, selected) {
                if before.has_permit() && complete_notice {
                    // Production must retain the selected hub before releasing
                    // the claim; only this independent signal may follow it.
                    self.route.signal.notify();
                }
                return;
            }
            before = self.load();
            assert_eq!(before.with_permit(false), claimed);
        }
    }

    fn publication(&self, generation: u64) -> Publication {
        loop {
            let word = self.load();
            if word.generation() != generation || word.phase() == Phase::Idle {
                return Publication::Stale;
            }
            if word.selected_cause().is_some() {
                return Publication::Published;
            }
            assert!(
                word.is_claimed(),
                "a routed notice requires a selected generation"
            );
            if word.has_permit() || self.replace(word, word.with_permit(true)) {
                return Publication::InFlight;
            }
        }
    }

    fn try_abandon(&self, generation: u64) -> bool {
        loop {
            let word = self.load();
            if word.generation() != generation || word.phase() == Phase::Idle {
                return true;
            }
            if word.is_claimed() {
                match self.publication(generation) {
                    Publication::InFlight => return false,
                    _ => continue,
                }
            }
            assert_ne!(word.phase(), Phase::Binding, "owner has already parked");
            if self.replace(word, word.with_resource(None).retire()) {
                if word.resource().is_some() {
                    assert_eq!(self.ownership.swap(0, Ordering::AcqRel), 1);
                    self.recovered.fetch_add(1, Ordering::Relaxed);
                }
                return true;
            }
        }
    }

    fn consume(&self, generation: u64) {
        let word = self.load();
        assert_eq!(word.generation(), generation);
        assert!(
            word.selected_cause().is_some(),
            "never mount an incomplete claim"
        );
        assert!(
            !word.has_permit(),
            "interest must not become a stored wake permit"
        );
        assert!(self.replace(word, word.with_resource(None).retire()));
        if word.resource().is_some() {
            assert_eq!(self.ownership.swap(0, Ordering::AcqRel), 1);
        }
        self.consumed.fetch_add(1, Ordering::Relaxed);
    }

    fn drive(&self, abandon: bool) {
        // One fixed deferred record, retaining exact generation and route owner.
        let mut deferred = None;
        loop {
            let observed = self.route.signal.version();
            if let Some(generation) = self.route.pop() {
                assert!(deferred.is_none_or(|old| old == generation));
                deferred = Some(generation);
            }
            if abandon && self.try_abandon(41) {
                return;
            }
            if let Some(generation) = deferred {
                match self.publication(generation) {
                    Publication::Published => {
                        if abandon {
                            assert!(self.try_abandon(generation));
                        } else {
                            self.consume(generation);
                        }
                        return;
                    }
                    Publication::InFlight => {}
                    Publication::Stale => panic!("live generation retired without its owner"),
                }
            }
            self.route.wait(observed);
        }
    }
}
