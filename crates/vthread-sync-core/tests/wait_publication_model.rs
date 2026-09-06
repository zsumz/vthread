//! Production publication and bounded MPSC routing under modeled atomics.
//!
//! WaitWord, owner publication/retirement and WakeQueue are imported source.
//! Signal retains its explicit SC adapter limitation. Native target binding,
//! ancestor stack destruction and complete multi-scope scheduling are not modeled.

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
#[path = "support/publication_routes_test.rs"]
mod publication_routes_test;
#[path = "support/publication_signal_test.rs"]
mod publication_signal;
#[path = "../../vthread/src/wait_state.rs"]
mod wait_state;

use publication_signal::Route;
use wait_state::{Phase, WaitWord};

fn model(f: impl Fn() + Send + Sync + 'static) -> usize {
    let mut builder = loom::model::Builder::new();
    builder.max_threads = 3;
    builder.max_branches = 1_000;
    builder.max_permutations = None;
    builder.max_duration = None;
    builder.preemption_bound = None;
    builder.checkpoint_file = None;
    // Harness bookkeeping only, after all protocol actors have joined. This
    // counter is not visible to any modeled decision or synchronization path.
    let completed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = std::sync::Arc::clone(&completed);
    builder.check(move || {
        f();
        count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    });
    completed.load(std::sync::atomic::Ordering::Relaxed)
}

#[path = "support/publication_types_test.rs"]
mod publication_types;
use publication_types::{ParkToken, TaskId, TaskKey, WakeNotice};

type WaitInner = Handoff;
type HubHandle = loom::sync::Arc<Route>;
#[path = "../../vthread/src/wait_owner_protocol.rs"]
mod owner_protocol;
use owner_protocol::Publication;
#[path = "../../vthread/src/wake_queue_core.rs"]
mod wake_queue_core;

struct Handoff {
    id: u64,
    word: AtomicU64,
    route: HubHandle,
    ownership: AtomicUsize,
    recovered: AtomicUsize,
    consumed: AtomicUsize,
}

impl Handoff {
    fn new() -> Self {
        Self::with_route(loom::sync::Arc::new(Route::new()), 1)
    }

    fn with_route(route: HubHandle, id: u64) -> Self {
        Self {
            id,
            word: AtomicU64::new(Self::active(41).raw()),
            route,
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

    fn compare_exchange(&self, before: WaitWord, after: WaitWord) -> Result<(), WaitWord> {
        self.word
            .compare_exchange(
                before.raw(),
                after.raw(),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map(|_| ())
            .map_err(WaitWord::from_raw)
    }

    fn replace(&self, before: WaitWord, after: WaitWord) -> bool {
        self.compare_exchange(before, after).is_ok()
    }

    fn clone_hub(&self, _: WaitWord) -> HubHandle {
        loom::sync::Arc::clone(&self.route)
    }

    fn observe_deferred(&self, _: ParkToken) {}
    fn observe_completion(&self, _: ParkToken) {}
    fn observe_retirement(&self, _: ParkToken) {}

    fn published(&self, generation: u64) -> Publication {
        self.publication(ParkToken::new(self.id, generation))
    }

    fn route_claim(&self, generation: u64) {
        self.route.push(WakeNotice {
            token: ParkToken::new(self.id, generation),
            task: TaskId::new(self.id),
            route: TaskKey::owned(self.id as usize - 1),
            cause: self.load().publish_claim().selected_cause().unwrap(),
        });
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
        if complete_notice {
            // The shipped candidate's exact CAS and hub-retention protocol.
            self.publish_claim(claimed);
            return;
        }
        // Deliberately broken negative control: keep the same state transition,
        // but omit the independent completion signal.
        let mut before = claimed;
        loop {
            if self.replace(before, before.publish_claim().with_permit(false)) {
                return;
            }
            before = self.load();
            assert_eq!(before.with_permit(false), claimed);
        }
    }

    fn try_abandon(&self, generation: u64) -> bool {
        if self
            .try_retire(ParkToken::new(self.id, generation))
            .is_err()
        {
            return false;
        }
        // Production retirement retains the selected resource for Ticket::drop.
        // Model that separate cleanup boundary, rather than retiring ownership
        // inside the publication protocol.
        loop {
            let word = self.load();
            if word.generation() != generation || word.resource().is_none() {
                return true;
            }
            if self.replace(word, word.with_resource(None)) {
                assert_eq!(self.ownership.swap(0, Ordering::AcqRel), 1);
                self.recovered.fetch_add(1, Ordering::Relaxed);
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
                match self.published(generation) {
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
