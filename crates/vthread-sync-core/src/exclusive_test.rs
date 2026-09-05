use std::sync::Arc;

use super::{ExclusiveCell, OwnershipSlot, QueueDecision};

#[test]
fn guards_exclude_and_preserve_mutation() {
    let cell = ExclusiveCell::new(0);
    let mut guard = cell.try_lock().expect("initial owner");
    assert!(cell.try_lock().is_none());
    *guard = 42;
    drop(guard);
    assert_eq!(*cell.try_lock().expect("released owner"), 42);
}

#[test]
fn queued_handoff_never_passes_through_unlocked() {
    let cell = ExclusiveCell::new(0);
    let guard = cell.try_lock().expect("initial owner");
    assert!(matches!(cell.queue_or_lock(), QueueDecision::Queued));
    let ownership = guard.try_release().expect_err("published waiter");
    assert!(cell.try_lock().is_none());
    assert!(cell.set_waiters(&ownership, false));
    let mut successor = cell.claim(ownership).ok().expect("exact successor");
    *successor = 42;
    drop(successor);
    assert_eq!(*cell.try_lock().expect("successor released"), 42);
}

#[test]
fn a_foreign_cell_cannot_claim_or_release_ownership() {
    let original = ExclusiveCell::new(1);
    let foreign = ExclusiveCell::new(2);
    let guard = original.try_lock().expect("initial owner");
    assert!(matches!(original.queue_or_lock(), QueueDecision::Queued));
    let ownership = guard.try_release().expect_err("waiter marker is required");
    let Err(ownership) = foreign.claim(ownership) else {
        panic!("foreign claim succeeded");
    };
    let ownership = foreign.unlock(ownership).expect_err("foreign release");
    assert!(original.unlock(ownership).is_ok());
    assert!(original.try_lock().is_some());
}

#[test]
fn ownership_can_move_to_the_selected_thread() {
    let cell = Arc::new(ExclusiveCell::new(0));
    let guard = cell.try_lock().expect("initial owner");
    assert!(matches!(cell.queue_or_lock(), QueueDecision::Queued));
    let ownership = guard.try_release().expect_err("published waiter");
    let target = Arc::clone(&cell);
    std::thread::spawn(move || {
        assert!(target.set_waiters(&ownership, false));
        *target.claim(ownership).ok().expect("selected owner") = 42;
    })
    .join()
    .unwrap();
    assert_eq!(*cell.try_lock().expect("thread released"), 42);
}

#[test]
fn ownership_slot_transfers_the_capability_exactly_once() {
    let cell = ExclusiveCell::new(0);
    let guard = cell.try_lock().expect("initial owner");
    assert!(matches!(cell.queue_or_lock(), QueueDecision::Queued));
    let ownership = guard.try_release().expect_err("published waiter");
    let slot = OwnershipSlot::new();

    assert!(slot.publish(ownership).is_ok());
    let ownership = slot.take().expect("published ownership");
    assert!(slot.take().is_none());
    assert!(cell.unlock(ownership).is_ok());
}

#[test]
fn racing_enqueue_and_release_never_lose_the_owner_or_value() {
    use std::sync::{Barrier, mpsc};

    for iteration in 0..256 {
        let cell = ExclusiveCell::new(0);
        let mut owner = cell.try_lock().unwrap();
        *owner = iteration;
        let barrier = Barrier::new(2);
        let (sender, receiver) = mpsc::sync_channel::<Option<super::Ownership>>(1);
        std::thread::scope(|scope| {
            let cell = &cell;
            let barrier = &barrier;
            let successor = scope.spawn(move || {
                barrier.wait();
                let mut guard = match cell.queue_or_lock() {
                    QueueDecision::Acquired(guard) => {
                        assert!(receiver.recv().unwrap().is_none());
                        guard
                    }
                    QueueDecision::Queued => {
                        let ownership = receiver.recv().unwrap().expect("selected owner");
                        assert!(cell.set_waiters(&ownership, false));
                        cell.claim(ownership).ok().expect("exact selected owner")
                    }
                };
                assert_eq!(*guard, iteration);
                *guard += 1;
            });
            barrier.wait();
            assert!(sender.send(owner.try_release().err()).is_ok());
            successor.join().unwrap();
        });
        assert_eq!(*cell.try_lock().unwrap(), iteration + 1);
    }
}
