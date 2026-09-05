use super::WakeQueue;
use crate::{
    TaskId,
    task_slab::TaskKey,
    wait::{WakeCause, WakeNotice},
};
use std::{collections::BTreeSet, sync::Arc, thread};
use vthread_stack::ParkToken;

fn notice(index: usize) -> WakeNotice {
    WakeNotice {
        token: ParkToken::new(index as u64 + 1, 1),
        task: TaskId::new(index as u64 + 1),
        route: TaskKey::owned(index),
        cause: WakeCause::Ready,
    }
}

#[test]
fn producer_and_consumer_cursors_do_not_share_a_cache_line() {
    let queue = WakeQueue::new(8);
    let producer = std::ptr::from_ref(&queue.head) as usize;
    let consumer = std::ptr::from_ref(&queue.consumer) as usize;
    assert!(producer.abs_diff(consumer) >= 64);
}

#[test]
fn wake_slots_fit_two_per_cache_line() {
    assert_eq!(std::mem::size_of::<super::Slot>(), 32);
    assert_eq!(std::mem::size_of::<super::Cursor>(), 64);
}

#[test]
fn distinct_routes_from_concurrent_producers_are_delivered_once() {
    let queue = Arc::new(WakeQueue::new(8));
    thread::scope(|scope| {
        for index in 0..8 {
            let queue = Arc::clone(&queue);
            scope.spawn(move || assert!(queue.push(notice(index), || {}).is_ok()));
        }
    });
    let mut delivered = BTreeSet::new();
    while let Some(notice) = queue.pop() {
        assert!(delivered.insert(notice.route.encoded()));
    }
    assert_eq!(delivered.len(), 8);
}

#[test]
fn a_consumed_route_can_be_republished_while_an_older_batch_remains() {
    let queue = WakeQueue::new(2);
    queue.push(notice(0), || {}).unwrap();
    queue.push(notice(1), || {}).unwrap();

    assert_eq!(queue.pop().unwrap().route, TaskKey::owned(0));
    let replacement = WakeNotice {
        token: ParkToken::new(3, 2),
        task: TaskId::new(3),
        route: TaskKey::owned(0),
        cause: WakeCause::Cancelled,
    };
    queue.push(replacement, || {}).unwrap();

    assert_eq!(queue.pop().unwrap().route, TaskKey::owned(1));
    assert_eq!(queue.pop(), Some(replacement));
    assert!(queue.pop().is_none());
}

#[test]
fn pending_counts_publications_rejections_and_reused_routes_at_every_capacity() {
    for capacity in [1, 64, 256, 257, 4096, 65536] {
        let queue = WakeQueue::new(capacity);
        let routes = capacity.min(8);
        for generation in 1..=4 {
            for index in 0..routes {
                let mut notice = notice(index);
                notice.token = ParkToken::new(notice.token.wait(), generation);
                queue.push(notice, || {}).unwrap();
                assert!(queue.push(notice, || {}).is_err());
                assert_eq!(queue.pending(), index + 1);
            }
            for remaining in (0..routes).rev() {
                assert_eq!(queue.pop().unwrap().token.generation(), generation);
                assert_eq!(queue.pending(), remaining);
            }
            assert!(queue.pop().is_none());
            assert_eq!(queue.pending(), 0);
        }
        assert!(queue.push(notice(capacity), || {}).is_err());
        assert_eq!(queue.pending(), 0);
    }
}

#[test]
fn publication_count_includes_reserved_but_not_yet_published_routes() {
    for capacity in [64, 4096] {
        let queue = WakeQueue::new(capacity);
        queue
            .push(notice(0), || {
                assert_eq!(queue.pending(), 1);
                assert!(!queue.has_pending());
            })
            .unwrap();
        assert_eq!(queue.pending(), 1);
        assert_eq!(queue.pop(), Some(notice(0)));
        assert_eq!(queue.pending(), 0);
    }
}

#[test]
fn concurrent_route_reuse_never_overcounts_or_underflows_pending() {
    use std::{
        sync::atomic::{AtomicBool, Ordering},
        time::{Duration, Instant},
    };
    let routes = [0, 1, 15, 30, 31, 32, 47, 63];
    let queue = WakeQueue::new(64);
    let stop = AtomicBool::new(false);
    let mut delivered = BTreeSet::new();
    let mut maximum_pending = 0;
    let mut duplicate_wakes = 0;
    let mut invalid_notice = false;
    thread::scope(|threads| {
        for route in routes {
            let queue = &queue;
            let stop = &stop;
            threads.spawn(move || {
                for generation in 1..=1024 {
                    let mut next = notice(route);
                    next.token = ParkToken::new(next.token.wait(), generation);
                    loop {
                        if stop.load(Ordering::Relaxed) {
                            return;
                        }
                        if queue.push(next, || {}).is_ok() {
                            break;
                        }
                        thread::yield_now();
                    }
                }
            });
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while delivered.len() != 8 * 1024 && Instant::now() < deadline {
            maximum_pending = maximum_pending.max(queue.pending());
            if let Some(wake) = queue.pop() {
                let route = wake.route.index();
                let generation = wake.token.generation();
                let mut expected = notice(route);
                expected.token = ParkToken::new(expected.token.wait(), generation);
                invalid_notice |= !routes.contains(&route)
                    || !(1..=1024).contains(&generation)
                    || wake != expected;
                duplicate_wakes +=
                    usize::from(!delivered.insert((wake.route.encoded(), generation)));
            } else {
                thread::yield_now();
            }
        }
        stop.store(true, Ordering::Relaxed);
    });
    assert_eq!(delivered.len(), 8 * 1024);
    assert_eq!(duplicate_wakes, 0);
    assert!(!invalid_notice, "received a notice that was never issued");
    assert!(
        maximum_pending <= 8,
        "observed {maximum_pending} pending wakes"
    );
    assert_eq!(queue.pending(), 0);
    assert!(queue.pop().is_none());
}

#[test]
fn both_route_kinds_preserve_every_wake_cause_and_packed_generation_boundary() {
    let queue = WakeQueue::new(32);
    for route in [
        TaskKey::owned(0),
        TaskKey::borrowed(0),
        TaskKey::owned(30),
        TaskKey::borrowed(30),
        TaskKey::owned(31),
        TaskKey::borrowed(31),
    ] {
        for cause in [
            WakeCause::Ready,
            WakeCause::TimedOut,
            WakeCause::Cancelled,
            WakeCause::InheritedCancelled,
            WakeCause::Closed,
        ] {
            for generation in [1, u64::MAX >> 3] {
                let wake = WakeNotice {
                    token: ParkToken::new(u64::MAX, generation),
                    task: TaskId::new(u64::MAX),
                    route,
                    cause,
                };
                assert!(queue.push(wake, || {}).is_ok());
                assert!(queue.push(wake, || {}).is_err());
                assert_eq!(queue.pop(), Some(wake));
                assert_eq!(queue.pop(), None);
            }
        }
    }
}
