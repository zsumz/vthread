use std::time::{Duration, Instant};

use vthread_stack::ParkToken;

use super::TimerQueue;

#[test]
fn deadlines_are_returned_in_monotonic_order() {
    let now = Instant::now();
    let early = ParkToken::new(1, 1);
    let late = ParkToken::new(2, 1);
    let mut timers = TimerQueue::new();
    assert!(timers.schedule(late, now + Duration::from_secs(2)));
    assert!(timers.schedule(early, now + Duration::from_secs(1)));

    assert_eq!(timers.next_deadline(), Some(now + Duration::from_secs(1)));
    assert_eq!(
        timers.pop_expired(now + Duration::from_millis(1500)),
        vec![early]
    );
    assert_eq!(timers.active_count(), 1);
    assert_eq!(timers.pop_expired(now + Duration::from_secs(3)), vec![late]);
}

#[test]
fn cancellation_prunes_stale_heap_entries() {
    let now = Instant::now();
    let cancelled = ParkToken::new(1, 1);
    let active = ParkToken::new(2, 1);
    let mut timers = TimerQueue::new();
    timers.schedule(cancelled, now + Duration::from_secs(1));
    timers.schedule(active, now + Duration::from_secs(2));
    assert!(timers.cancel(cancelled));

    assert_eq!(timers.next_deadline(), Some(now + Duration::from_secs(2)));
    assert_eq!(
        timers.pop_expired(now + Duration::from_secs(3)),
        vec![active]
    );
    assert_eq!(timers.active_count(), 0);
}
