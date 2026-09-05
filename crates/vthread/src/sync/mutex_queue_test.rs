use super::{MutexQueue, Subscription};
use crate::{
    Error, TaskId,
    signal::lock,
    task_slab::TaskKey,
    wait::{WaitBegin, WaitCell, WaitHub, WakeCause},
};
use std::sync::{Arc, mpsc};
use vthread_sync_core::ExclusiveCell;

#[test]
fn subscription_closes_an_unlock_before_enqueue_race() {
    let queue = MutexQueue::new(1).unwrap();
    let value = ExclusiveCell::new(42);
    drop(value.try_lock().expect("first owner"));

    let wait = WaitCell::new();
    let Subscription::Acquired(guard) = queue.subscribe(&value, &wait).unwrap() else {
        panic!("released mutex queued a waiter");
    };
    assert_eq!(*guard, 42);
    assert_eq!(queue.waiting(), 0);
}

#[test]
fn abandoned_selected_ticket_returns_ownership() {
    let queue = MutexQueue::new(1).unwrap();
    let value = ExclusiveCell::new(0);
    let owner = value.try_lock().expect("first owner");
    let wait = WaitCell::new();
    let Subscription::Waiting(ticket) = queue.subscribe(&value, &wait).unwrap() else {
        panic!("locked mutex admitted a new owner");
    };

    queue.release(&value, owner);
    assert!(value.try_lock().is_none());
    drop(ticket);
    assert!(value.try_lock().is_some());
    assert_eq!(queue.waiting(), 0);
}

#[test]
fn outstanding_tickets_remain_bounded() {
    let queue = MutexQueue::new(1).unwrap();
    let value = ExclusiveCell::new(0);
    let owner = value.try_lock().expect("first owner");
    let first_wait = WaitCell::new();
    let Subscription::Waiting(first) = queue.subscribe(&value, &first_wait).unwrap() else {
        panic!("locked mutex admitted a new owner");
    };
    let second_wait = WaitCell::new();

    assert!(matches!(
        queue.subscribe(&value, &second_wait),
        Err(Error::Capacity { limit: 1, .. })
    ));
    drop(first);
    queue.release(&value, owner);
    assert!(value.try_lock().is_some());
}

#[test]
fn selected_ticket_remains_part_of_the_wait_bound() {
    let queue = MutexQueue::new(1).unwrap();
    let value = ExclusiveCell::new(0);
    let owner = value.try_lock().expect("first owner");
    let selected_wait = WaitCell::new();
    let Subscription::Waiting(selected) = queue.subscribe(&value, &selected_wait).unwrap() else {
        panic!("locked mutex admitted a new owner");
    };

    queue.release(&value, owner);
    let rejected_wait = WaitCell::new();
    assert!(matches!(
        queue.subscribe(&value, &rejected_wait),
        Err(Error::Capacity { limit: 1, .. })
    ));

    drop(selected);
    assert!(value.try_lock().is_some());
    assert_eq!(queue.waiting(), 0);
}

#[test]
fn an_idle_ticket_can_return_ownership_before_deferred_publication() {
    let queue = MutexQueue::new(1).unwrap();
    let value = ExclusiveCell::new(42);
    std::thread::scope(|threads| {
        let (held_tx, held_rx) = mpsc::channel();
        let (unlock_tx, unlock_rx) = mpsc::channel();
        let (dequeued_tx, dequeued_rx) = mpsc::channel();
        let (publish_tx, publish_rx) = mpsc::channel();
        *lock(&queue.after_dequeue) = Some(Box::new(move || {
            let _ = dequeued_tx.send(());
            let _ = publish_rx.recv();
        }));
        let queue = &queue;
        let value = &value;
        let owner = threads.spawn(move || {
            let owner = value.try_lock().unwrap();
            let _ = held_tx.send(());
            let _ = unlock_rx.recv();
            queue.release(value, owner);
        });
        held_rx.recv().unwrap();
        let wait = WaitCell::new();
        let Subscription::Waiting(ticket) = queue.subscribe(value, &wait).unwrap() else {
            panic!("locked mutex admitted a new owner");
        };
        unlock_tx.send(()).unwrap();
        dequeued_rx.recv().unwrap();
        drop(ticket);
        assert_eq!(queue.waiting(), 0);
        assert_eq!(
            *value.try_lock().expect("idle grant returned ownership"),
            42
        );
        assert!(wait.recycle());
        publish_tx.send(()).unwrap();
        owner.join().unwrap();
        assert_eq!(wait.take_resource(), None);
        assert!(value.try_lock().is_some());
    });
}

#[test]
fn unwinding_after_queue_removal_still_publishes_the_selected_handoff() {
    let queue = MutexQueue::new(1).unwrap();
    let value = ExclusiveCell::new(42);
    let owner = value.try_lock().unwrap();
    let wait = WaitCell::new();
    let Subscription::Waiting(ticket) = queue.subscribe(&value, &wait).unwrap() else {
        panic!("locked mutex admitted a new owner");
    };
    let hub = Arc::new(WaitHub::new(1, Arc::default()));
    let WaitBegin::Park { request, .. } = wait
        .begin(TaskId::new(1), TaskKey::owned(0), &hub, None)
        .unwrap()
    else {
        panic!("expected a park")
    };
    *lock(&queue.after_dequeue) = Some(Box::new(|| panic!("publisher interrupted")));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            queue.release(&value, owner);
        }))
        .is_err()
    );
    assert_eq!(hub.pop_wake().unwrap().token, request.token());
    assert!(hub.pop_wake().is_none());
    drop(ticket);
    assert_eq!(wait.finish(request.token()).unwrap(), WakeCause::Ready);
    assert_eq!(queue.waiting(), 0);
    assert_eq!(
        *value.try_lock().expect("unwound grant returned ownership"),
        42
    );
}
