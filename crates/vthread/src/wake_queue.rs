//! Fixed-capacity MPSC wake routing with one owner-carrier consumer.

use std::sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering};

use vthread_stack::ParkToken;

use crate::{
    TaskId,
    task_slab::TaskKey,
    wait::{WakeCause, WakeNotice},
};

struct Slot {
    next: AtomicUsize,
    task: AtomicU64,
    wait: AtomicU64,
    generation: AtomicU64,
    cause: AtomicU8,
}

#[repr(align(64))]
struct Cursor(AtomicUsize);

impl Cursor {
    fn new() -> Self {
        Self(AtomicUsize::new(0))
    }
}

impl Slot {
    fn new() -> Self {
        Self {
            next: AtomicUsize::new(0),
            task: AtomicU64::new(0),
            wait: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            cause: AtomicU8::new(0),
        }
    }
}

pub(crate) struct WakeQueue {
    slots: Box<[Slot]>,
    head: Cursor,
    consumer: Cursor,
}

impl WakeQueue {
    const SLEEPING: usize = 1 << (usize::BITS - 1);
    const INDEX_MASK: usize = !Self::SLEEPING;

    pub(crate) fn new(capacity: usize) -> Self {
        let slot_count = capacity
            .checked_mul(2)
            .and_then(|count| count.checked_add(2))
            .expect("wake queue capacity exhausted");
        assert!(slot_count < Self::SLEEPING, "wake queue capacity exhausted");
        Self {
            slots: (0..slot_count).map(|_| Slot::new()).collect(),
            head: Cursor::new(),
            consumer: Cursor::new(),
        }
    }

    pub(crate) fn push(
        &self,
        notice: WakeNotice,
        before_publish: impl FnOnce(),
    ) -> std::result::Result<bool, WakeNotice> {
        let index = notice.route.encoded();
        let Some(slot) = self.slots.get(index) else {
            return Err(notice);
        };
        // Wait selection admits exactly one notice for a task route. The load
        // still rejects retained stale notices without putting a second route
        // node into the intrusive list; the release on `head` publishes the
        // slot fields to the owner consumer.
        if slot.task.load(Ordering::Acquire) != 0 {
            return Err(notice);
        }
        slot.task.store(notice.task.get(), Ordering::Relaxed);
        slot.wait.store(notice.token.wait(), Ordering::Relaxed);
        slot.generation
            .store(notice.token.generation(), Ordering::Relaxed);
        slot.cause
            .store(encode_cause(notice.cause), Ordering::Relaxed);
        before_publish();

        let mut head = self.head.0.load(Ordering::Acquire);
        loop {
            slot.next.store(head & Self::INDEX_MASK, Ordering::Relaxed);
            match self.head.0.compare_exchange_weak(
                head,
                index,
                Ordering::Release,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(head & Self::SLEEPING != 0),
                Err(observed) => head = observed,
            }
        }
    }

    pub(crate) fn pop(&self) -> Option<WakeNotice> {
        let mut index = self.consumer.0.load(Ordering::Relaxed);
        if index == 0 {
            // A one-item batch is the common handoff shape. Avoid a second
            // locked exchange when the caller probes once more for emptiness.
            if self.head.0.load(Ordering::Acquire) & Self::INDEX_MASK == 0 {
                return None;
            }
            index = self.reverse(self.head.0.swap(0, Ordering::AcqRel) & Self::INDEX_MASK);
            if index == 0 {
                return None;
            }
        }
        let slot = &self.slots[index];
        self.consumer
            .0
            .store(slot.next.load(Ordering::Relaxed), Ordering::Relaxed);
        let notice = WakeNotice {
            token: ParkToken::new(
                slot.wait.load(Ordering::Relaxed),
                slot.generation.load(Ordering::Relaxed),
            ),
            task: TaskId::new(slot.task.load(Ordering::Relaxed)),
            route: TaskKey::from_encoded(index),
            cause: decode_cause(slot.cause.load(Ordering::Relaxed)),
        };
        slot.task.store(0, Ordering::Release);
        Some(notice)
    }

    pub(crate) fn has_pending(&self) -> bool {
        self.consumer.0.load(Ordering::Relaxed) != 0
            || self.head.0.load(Ordering::Acquire) & Self::INDEX_MASK != 0
    }

    pub(crate) fn pending(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.task.load(Ordering::Acquire) != 0)
            .count()
    }

    pub(crate) fn arm_wait(&self) -> bool {
        if self.consumer.0.load(Ordering::Relaxed) != 0 {
            return true;
        }
        let mut head = self.head.0.load(Ordering::Acquire);
        loop {
            if head & Self::INDEX_MASK != 0 {
                return true;
            }
            if head & Self::SLEEPING != 0 {
                return false;
            }
            match self.head.0.compare_exchange_weak(
                head,
                Self::SLEEPING,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return false,
                Err(observed) => head = observed,
            }
        }
    }

    pub(crate) fn disarm_wait(&self) {
        self.head.0.fetch_and(Self::INDEX_MASK, Ordering::Release);
    }

    fn reverse(&self, mut head: usize) -> usize {
        let mut reversed = 0;
        while head != 0 {
            let slot = &self.slots[head];
            let next = slot.next.load(Ordering::Relaxed);
            slot.next.store(reversed, Ordering::Relaxed);
            reversed = head;
            head = next;
        }
        reversed
    }
}

fn encode_cause(cause: WakeCause) -> u8 {
    match cause {
        WakeCause::Ready => 1,
        WakeCause::TimedOut => 2,
        WakeCause::Cancelled => 3,
        WakeCause::InheritedCancelled => 4,
        WakeCause::Closed => 5,
    }
}

fn decode_cause(cause: u8) -> WakeCause {
    match cause {
        1 => WakeCause::Ready,
        2 => WakeCause::TimedOut,
        3 => WakeCause::Cancelled,
        4 => WakeCause::InheritedCancelled,
        5 => WakeCause::Closed,
        _ => unreachable!("invalid queued wake cause"),
    }
}

#[cfg(test)]
#[path = "wake_queue_test.rs"]
mod wake_queue_test;
