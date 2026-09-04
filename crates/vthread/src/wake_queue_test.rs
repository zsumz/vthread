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
