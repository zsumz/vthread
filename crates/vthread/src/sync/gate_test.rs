use super::Gate;
use crate::{Error, signal::lock};

#[test]
fn selected_tickets_remain_bounded_and_abandoned_permits_follow_fifo() {
    let gate = Gate::new(0, 1, 2).unwrap();
    let first = gate.subscribe_test().unwrap();
    let second = gate.subscribe_test().unwrap();
    gate.signal();
    assert!(matches!(gate.try_take(), Err(Error::WouldBlock)));
    assert!(matches!(
        gate.subscribe_test(),
        Err(Error::Capacity {
            resource: crate::error::CapacityResource::Waiters,
            limit: 2
        })
    ));
    drop(first);
    assert!(lock(&gate.state).entries.is_empty());
    assert_eq!(gate.waiting(), 1);
    drop(second);
    assert_eq!(gate.waiting(), 0);
    gate.try_take().unwrap();
    assert!(matches!(gate.try_take(), Err(Error::WouldBlock)));
}

#[test]
fn cancelled_broadcasts_do_not_notify_future_waiters() {
    let gate = Gate::new(0, 1, 2).unwrap();
    let first = gate.subscribe_test().unwrap();
    gate.broadcast();
    let second = gate.subscribe_test().unwrap();
    drop(first);
    assert_eq!(lock(&gate.state).entries.len(), 1);
    drop(second);
    assert_eq!(gate.available(), 0);
}

#[test]
fn close_discards_selected_and_stored_permits() {
    let gate = Gate::new(0, 1, 1).unwrap();
    let ticket = gate.subscribe_test().unwrap();
    gate.signal();
    gate.close();
    drop(ticket);
    gate.signal();
    assert_eq!(gate.available(), 0);
    assert!(matches!(gate.try_take(), Err(Error::Closed)));
    assert!(matches!(gate.subscribe_test(), Err(Error::Closed)));
}

#[test]
fn abandoned_tickets_release_capacity_and_leave_no_queue_entry() {
    let gate = Gate::new(0, 1, 2).unwrap();
    let first = gate.subscribe_test().unwrap();
    drop(first);
    assert!(lock(&gate.state).entries.is_empty());
    assert_eq!(gate.waiting(), 0);

    let second = gate.subscribe_test().unwrap();
    drop(second);
    assert!(lock(&gate.state).entries.is_empty());
    assert_eq!(gate.waiting(), 0);
}
