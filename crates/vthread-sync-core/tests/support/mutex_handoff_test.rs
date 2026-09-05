use super::*;
use loom::{sync::Arc, thread};

fn cancellation_race<const SELECT_UNDER_LOCK: bool>(cause: WakeCause) {
    model(move || {
        let state = Arc::new(Handoff::new());
        let owner = Arc::clone(&state);
        let owner = thread::spawn(move || owner.release::<SELECT_UNDER_LOCK>());
        let recipient = Arc::clone(&state);
        let recipient = thread::spawn(move || recipient.cancel_and_drop(cause));
        owner.join().unwrap();
        recipient.join().unwrap();
        state.assert_complete();
    });
}

#[test]
#[should_panic(expected = "ownership leaked")]
fn negative_control_finds_the_old_dequeue_before_selection_leak() {
    cancellation_race::<false>(WakeCause::Cancelled);
}

#[test]
fn selection_under_queue_lock_returns_ownership_for_every_cancellation_order() {
    cancellation_race::<true>(WakeCause::Cancelled);
}

#[test]
fn selection_under_queue_lock_returns_ownership_for_inherited_cancellation() {
    cancellation_race::<true>(WakeCause::InheritedCancelled);
}

#[test]
fn selection_under_queue_lock_returns_ownership_for_timeout() {
    cancellation_race::<true>(WakeCause::TimedOut);
}

#[test]
fn selection_under_queue_lock_returns_ownership_for_close() {
    cancellation_race::<true>(WakeCause::Closed);
}

#[test]
fn resource_cleanup_waits_for_write_exclusive_claim_publication() {
    model(|| {
        let state = Arc::new(Handoff::new());
        let publisher = Arc::clone(&state);
        let publisher = thread::spawn(move || {
            let claimed = publisher.reserve_resource().unwrap().unwrap();
            publisher.publish(claimed);
        });
        let consumer = Arc::clone(&state);
        let consumer = thread::spawn(move || {
            loop {
                if consumer.take_resource().is_some() {
                    break;
                }
                thread::yield_now();
            }
        });
        publisher.join().unwrap();
        consumer.join().unwrap();
        assert_eq!(state.load().phase(), Phase::SelectedReady);
        assert_eq!(state.load().resource(), None);
        assert_eq!(state.wakes.load(Ordering::Relaxed), 1);
    });
}
