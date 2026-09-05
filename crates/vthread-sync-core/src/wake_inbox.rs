//! Test-only routing prototype: reserved payloads, inline mailbox, bounded overflow.
//!
//! The measured runtime integration regressed handoff throughput and was removed.
//! Keep this implementation shared by standard and modeled composition tests.

use super::{
    wake_atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    wake_mailbox::WakeMailbox,
};

const SLEEPING: usize = 1 << (usize::BITS - 1);
const INDEX_MASK: usize = !SLEEPING;

struct Slot {
    next: AtomicUsize,
    task: AtomicU64,
    wait: AtomicU64,
    selection: AtomicU64,
}

#[repr(align(64))]
struct Head(AtomicUsize);

#[repr(align(64))]
struct Consumer {
    next: AtomicUsize,
    overflow_first: AtomicBool,
}

/// Opaque task and generation metadata. The scheduler validates it before resuming.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WakePacket {
    pub(crate) route: usize,
    pub(crate) task: u64,
    pub(crate) wait: u64,
    pub(crate) selection: u64,
}

/// Fixed-capacity MPSC routing with exactly one owner consumer.
///
/// The caller's wait arbitration must exclude concurrent publishers of the same
/// route. A nonzero task field then reserves the slot until the owner has copied
/// every payload field. This is not a general-purpose MPSC queue for arbitrary
/// same-route producers. The scheduler still validates task and park generations.
/// `pop`, `has_pending`, and sleep operations belong to the owner. `pending` is a
/// diagnostic scan, including reserved but not yet published payloads.
pub(crate) struct WakeInbox {
    slots: Box<[Slot]>,
    mailbox: WakeMailbox,
    head: Head,
    consumer: Consumer,
}

impl WakeInbox {
    /// Allocates exactly one payload slot per encoded route, once at construction.
    pub(crate) fn new(slot_count: usize) -> Self {
        assert!(slot_count < SLEEPING, "wake queue capacity exhausted");
        Self {
            slots: (0..slot_count)
                .map(|_| Slot {
                    next: AtomicUsize::new(0),
                    task: AtomicU64::new(0),
                    wait: AtomicU64::new(0),
                    selection: AtomicU64::new(0),
                })
                .collect(),
            mailbox: WakeMailbox::new(),
            head: Head(AtomicUsize::new(0)),
            consumer: Consumer {
                next: AtomicUsize::new(0),
                overflow_first: AtomicBool::new(false),
            },
        }
    }

    /// Publishes one uniquely selected wake, returning whether to notify the owner.
    /// Rejects an invalid route, a zero task identity, or an already reserved route.
    /// The hook must not panic or publish the same route recursively.
    #[inline]
    pub(crate) fn push(
        &self,
        packet: WakePacket,
        before_publish: impl FnOnce(),
    ) -> Result<bool, WakePacket> {
        let Some(slot) = self.slots.get(packet.route) else {
            return Err(packet);
        };
        if packet.task == 0 || slot.task.load(Ordering::Acquire) != 0 {
            return Err(packet);
        }
        slot.task.store(packet.task, Ordering::Relaxed);
        slot.wait.store(packet.wait, Ordering::Relaxed);
        slot.selection.store(packet.selection, Ordering::Relaxed);
        before_publish();
        if packet.route < WakeMailbox::ROUTES {
            return Ok(self.mailbox.publish(packet.route));
        }
        let mut head = self.head.0.load(Ordering::Acquire);
        loop {
            slot.next.store(head & INDEX_MASK, Ordering::Relaxed);
            match self.head.0.compare_exchange_weak(
                head,
                packet.route,
                Ordering::Release,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(head & SLEEPING != 0),
                Err(observed) => head = observed,
            }
        }
    }

    /// Copies and releases one payload, alternating lanes when both remain ready.
    /// Each lane drains its captured batch before accepting a fresh batch.
    #[inline]
    pub(crate) fn pop(&self) -> Option<WakePacket> {
        let (route, overflow) = if self.consumer.overflow_first.load(Ordering::Relaxed) {
            self.pop_overflow()
                .map(|route| (route, true))
                .or_else(|| self.mailbox.pop().map(|route| (route, false)))?
        } else {
            self.mailbox
                .pop()
                .map(|route| (route, false))
                .or_else(|| self.pop_overflow().map(|route| (route, true)))?
        };
        self.consumer
            .overflow_first
            .store(!overflow, Ordering::Relaxed);
        let slot = &self.slots[route];
        let packet = WakePacket {
            route,
            task: slot.task.load(Ordering::Relaxed),
            wait: slot.wait.load(Ordering::Relaxed),
            selection: slot.selection.load(Ordering::Relaxed),
        };
        // The mailbox acknowledgement and every payload read precede republication.
        slot.task.store(0, Ordering::Release);
        Some(packet)
    }

    /// An owner-side readiness hint without a scan or locked RMW.
    #[inline]
    pub(crate) fn has_pending(&self) -> bool {
        self.mailbox.has_pending()
            || self.consumer.next.load(Ordering::Relaxed) != 0
            || self.head.0.load(Ordering::Acquire) & INDEX_MASK != 0
    }

    /// Observes all reserved slots, including a producer paused before publication.
    pub(crate) fn pending(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.task.load(Ordering::Acquire) != 0)
            .count()
    }

    /// Finds ready work or arms both lanes after the caller registers for notification.
    pub(crate) fn arm_wait(&self) -> bool {
        if self.mailbox.arm_wait() || self.consumer.next.load(Ordering::Relaxed) != 0 {
            return true;
        }
        let mut head = self.head.0.load(Ordering::Acquire);
        loop {
            if head & INDEX_MASK != 0 {
                return true;
            }
            if head & SLEEPING != 0 {
                return false;
            }
            match self.head.0.compare_exchange_weak(
                head,
                SLEEPING,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return false,
                Err(observed) => head = observed,
            }
        }
    }

    /// Ends sleep intent without clearing either lane's pending work.
    pub(crate) fn disarm_wait(&self) {
        self.mailbox.disarm_wait();
        self.head.0.fetch_and(INDEX_MASK, Ordering::Release);
    }

    #[inline]
    fn pop_overflow(&self) -> Option<usize> {
        let mut route = self.consumer.next.load(Ordering::Relaxed);
        if route == 0 {
            if self.head.0.load(Ordering::Acquire) & INDEX_MASK == 0 {
                return None;
            }
            route = self.take_overflow_batch();
            if route == 0 {
                return None;
            }
        }
        self.consumer.next.store(
            self.slots[route].next.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        Some(route)
    }

    #[cold]
    fn take_overflow_batch(&self) -> usize {
        let route = self.head.0.swap(0, Ordering::Acquire) & INDEX_MASK;
        if route == 0 || self.slots[route].next.load(Ordering::Relaxed) == 0 {
            route
        } else {
            self.reverse(route)
        }
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

#[cfg(test)]
#[path = "wake_inbox_test.rs"]
mod wake_inbox_test;
