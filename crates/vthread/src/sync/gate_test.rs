use super::Gate;
use crate::{Error, signal::lock};

#[test]
fn selected_tickets_remain_bounded_and_abandoned_permits_follow_fifo() {
    let gate = Gate::new(0, 1, 2).unwrap();
    let first = gate.subscribe().unwrap();
    let second = gate.subscribe().unwrap();
    gate.signal();
    assert!(matches!(gate.try_take(), Err(Error::WouldBlock)));
    assert!(matches!(
        gate.subscribe(),
        Err(Error::Capacity {
            resource: crate::error::CapacityResource::Waiters,
            limit: 2
        })
    ));
    drop(first);
    assert_eq!(
        lock(&gate.state).entries.front().unwrap().granted,
        Some(true)
    );
    drop(second);
    assert_eq!(gate.waiting(), 0);
    gate.try_take().unwrap();
    assert!(matches!(gate.try_take(), Err(Error::WouldBlock)));
}

#[test]
fn cancelled_broadcasts_do_not_notify_future_waiters() {
    let gate = Gate::new(0, 1, 2).unwrap();
    let first = gate.subscribe().unwrap();
    gate.broadcast();
    let second = gate.subscribe().unwrap();
    drop(first);
    assert_eq!(lock(&gate.state).entries.front().unwrap().granted, None);
    drop(second);
    assert_eq!(gate.available(), 0);
}

#[test]
fn close_discards_selected_and_stored_permits() {
    let gate = Gate::new(0, 1, 1).unwrap();
    let ticket = gate.subscribe().unwrap();
    gate.signal();
    gate.close();
    drop(ticket);
    gate.signal();
    assert_eq!(gate.available(), 0);
    assert!(matches!(gate.try_take(), Err(Error::Closed)));
    assert!(matches!(gate.subscribe(), Err(Error::Closed)));
}

#[test]
fn abandoned_tickets_reuse_the_wait_cache_high_water() {
    let gate = Gate::new(0, 1, 2).unwrap();
    let first = gate.subscribe().unwrap();
    let identity = first.parker().wait.identity();
    drop(first);
    assert_eq!(lock(&gate.state).vacant.len(), 1);

    let second = gate.subscribe().unwrap();
    assert_eq!(second.parker().wait.identity(), identity);
    drop(second);
    assert_eq!(lock(&gate.state).vacant.len(), 1);
}
