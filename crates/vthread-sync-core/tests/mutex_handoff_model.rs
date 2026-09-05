//! Model the runtime queue/claim/cleanup composition, without switching stacks.
//!
//! `WaitWord`, resource decisions and claim/retirement encoding are the production
//! source. The adapter below preserves MutexQueue's lock boundary, WaitInner's
//! AcqRel/Acquire CAS and Release publication, and OwnershipSlot's Release/Acquire
//! transfer. The queue has one bounded entry; routed wakes are counted, not scheduled.
//! Native tests separately exercise the real queue, checkpoints, routing and unwind.

#![forbid(unsafe_code)]

use loom::sync::{
    Mutex,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

// Type-only stand-ins avoid importing the native runtime into the model. The
// production word's sibling tests also check every cause and resource encoding.
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

#[path = "../../vthread/src/wait_state.rs"]
mod wait_state;
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

struct Handoff {
    queued: Mutex<bool>,
    slot: AtomicU64,
    word: AtomicU64,
    wakes: AtomicUsize,
    returned: AtomicUsize,
}

impl Handoff {
    fn new() -> Self {
        Self {
            queued: Mutex::new(true),
            slot: AtomicU64::new(0),
            word: AtomicU64::new(
                WaitWord::initial()
                    .with_generation(41)
                    .with_phase(Phase::Active)
                    .raw(),
            ),
            wakes: AtomicUsize::new(0),
            returned: AtomicUsize::new(0),
        }
    }

    fn load(&self) -> WaitWord {
        WaitWord::from_raw(self.word.load(Ordering::Acquire))
    }

    fn replace(&self, old: WaitWord, next: WaitWord) -> bool {
        self.word
            .compare_exchange(old.raw(), next.raw(), Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    fn publish(&self, claimed: WaitWord) {
        assert_eq!(self.load(), claimed, "claim lost write exclusivity");
        self.wakes.fetch_add(1, Ordering::Relaxed);
        self.word
            .store(claimed.publish_claim().raw(), Ordering::Release);
    }

    fn reserve_resource(&self) -> Option<Option<WaitWord>> {
        loop {
            let word = self.load();
            if word.phase() == Phase::Binding {
                loom::thread::yield_now();
                continue;
            }
            let next = word.resource_offer(ResourceSelection::Permit)?;
            if self.replace(word, next) {
                return Some((word.phase() == Phase::Active).then_some(next));
            }
        }
    }

    fn take_resource(&self) -> Option<ResourceSelection> {
        loop {
            let word = self.load();
            word.resource()?;
            if let Some((next, resource)) = word.resource_take()
                && self.replace(word, next)
            {
                return Some(resource);
            }
            loom::thread::yield_now();
        }
    }

    fn return_ownership(&self) {
        assert_eq!(
            self.returned.fetch_add(1, Ordering::Relaxed),
            0,
            "ownership returned twice"
        );
    }

    fn reclaim_ownership(&self) {
        assert_eq!(
            self.slot.swap(0, Ordering::Acquire),
            1,
            "recipient lacked ownership"
        );
        self.return_ownership();
    }

    fn release<const SELECT_UNDER_LOCK: bool>(&self) {
        let mut queued = self.queued.lock().unwrap();
        if !*queued {
            self.return_ownership();
            return;
        }
        *queued = false;
        assert_eq!(
            self.slot
                .compare_exchange(0, 1, Ordering::Release, Ordering::Relaxed),
            Ok(0)
        );
        let reserved = SELECT_UNDER_LOCK.then(|| self.reserve_resource());
        drop(queued);
        // false reproduces the old dequeue-before-offer window as a negative control.
        let reserved = reserved.unwrap_or_else(|| self.reserve_resource());
        if let Some(claimed) = reserved {
            if let Some(claimed) = claimed {
                self.publish(claimed);
            }
        } else {
            self.reclaim_ownership();
        }
    }

    fn cancel_and_drop(&self, cause: WakeCause) {
        loop {
            let word = self.load();
            if word.phase() != Phase::Active {
                break;
            }
            let claimed = if cause == WakeCause::Closed {
                word.with_closed(true).with_permit(false).claimed(cause)
            } else {
                word.claimed(cause)
            };
            if self.replace(word, claimed) {
                self.publish(claimed);
                break;
            }
        }
        // The real park path retires the winner before Ticket::drop. A Ready
        // winner still returns ownership when the post-resume checkpoint fails.
        let ready = loop {
            let word = self.load();
            if word.is_claimed() {
                loom::thread::yield_now();
                continue;
            }
            let ready = word.selected_cause().expect("selected wake") == WakeCause::Ready;
            let retired = if ready {
                word.with_resource(None).retire()
            } else {
                word.retire()
            };
            if self.replace(word, retired) {
                break ready;
            }
        };
        let queued = {
            let mut queued = self.queued.lock().unwrap();
            std::mem::replace(&mut *queued, false)
        };
        let selected =
            ready || (!queued && self.take_resource() == Some(ResourceSelection::Permit));
        if !queued && selected {
            self.reclaim_ownership();
        }
    }

    fn assert_complete(&self) {
        assert_eq!(self.returned.load(Ordering::Relaxed), 1, "ownership leaked");
        assert_eq!(self.slot.load(Ordering::Relaxed), 0);
        assert!(!*self.queued.lock().unwrap());
        assert_eq!(self.load().phase(), Phase::Idle);
        assert_eq!(self.load().resource(), None);
        assert_eq!(self.load().generation(), 41);
        assert_eq!(self.wakes.load(Ordering::Relaxed), 1);
    }
}

#[path = "support/mutex_handoff_test.rs"]
mod mutex_handoff_test;
