use super::{MutexQueue, Subscription};
use crate::{Error, wait::WaitCell};
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
