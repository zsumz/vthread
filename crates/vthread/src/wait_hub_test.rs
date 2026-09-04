use std::sync::Arc;

use vthread_stack::ParkToken;

use super::WaitHub;
use crate::{
    TaskId,
    task_slab::TaskKey,
    wait::{WakeCause, WakeNotice},
};

#[test]
fn duplicates_and_stale_generations_cannot_fill_the_inbox() {
    let hub = WaitHub::new_tracked(1, Arc::default());
    let token = ParkToken::new(1, 2);
    let notice = WakeNotice {
        token,
        task: TaskId::new(1),
        route: TaskKey::owned(0),
        cause: WakeCause::Ready,
    };
    hub.enqueue(notice);
    for _ in 0..100 {
        hub.enqueue(notice);
        hub.enqueue(WakeNotice {
            token: ParkToken::new(1, 1),
            ..notice
        });
    }
    assert_eq!(hub.pending(), 1);
    assert_eq!(hub.pending_tasks(), vec![TaskId::new(1)]);
    assert_eq!(hub.stale(), 200);
    assert_eq!(hub.pop_wake(), Some(notice));
    assert!(hub.pop_wake().is_none());
}

#[test]
fn queued_wakes_release_predicate_waiters_without_advancing_the_epoch() {
    let signal = Arc::default();
    let hub = WaitHub::new(2, Arc::clone(&signal));
    let first = ParkToken::new(1, 1);
    let second = ParkToken::new(2, 1);
    let empty = signal.version();
    std::thread::scope(|threads| {
        threads.spawn(|| {
            hub.wait(empty, None);
        });
        while signal.waiting() == 0 {
            std::thread::yield_now();
        }
        hub.enqueue(WakeNotice {
            token: first,
            task: TaskId::new(1),
            route: TaskKey::owned(0),
            cause: WakeCause::Ready,
        });
    });
    let queued = signal.version();
    assert_eq!(queued, empty);
    hub.enqueue(WakeNotice {
        token: second,
        task: TaskId::new(2),
        route: TaskKey::owned(1),
        cause: WakeCause::Ready,
    });
    assert_eq!(signal.version(), queued);
    assert!(hub.pop_wake().is_some());
    assert!(hub.pop_wake().is_some());
}

#[test]
fn published_predicate_never_precedes_its_queue_item() {
    let hub = Arc::new(WaitHub::new(1, Arc::default()));
    let (arrived_tx, arrived_rx) = std::sync::mpsc::channel();
    let (resume_tx, resume_rx) = std::sync::mpsc::channel();
    hub.before_pending_publication(move || {
        arrived_tx.send(()).unwrap();
        resume_rx.recv().unwrap();
    });
    let producer_hub = Arc::clone(&hub);
    let producer = std::thread::spawn(move || {
        producer_hub.enqueue(WakeNotice {
            token: ParkToken::new(1, 1),
            task: TaskId::new(1),
            route: TaskKey::owned(0),
            cause: WakeCause::Ready,
        });
    });

    arrived_rx.recv().unwrap();
    assert_eq!(hub.pending(), 1);
    assert!(!hub.ready.has_pending());
    resume_tx.send(()).unwrap();
    producer.join().unwrap();
    assert_eq!(hub.pending(), 1);
    assert!(hub.ready.has_pending());
    assert!(hub.pop_wake().is_some());
}

#[test]
fn wake_inbox_preserves_publication_order() {
    let hub = WaitHub::new(3, Arc::default());
    for index in 0..3 {
        hub.enqueue(WakeNotice {
            token: ParkToken::new(index as u64 + 1, 1),
            task: TaskId::new(index as u64 + 1),
            route: TaskKey::owned(index),
            cause: WakeCause::Ready,
        });
    }

    for expected in 1..=3 {
        assert_eq!(hub.pop_wake().unwrap().task, TaskId::new(expected));
    }
    assert!(hub.pop_wake().is_none());
}
