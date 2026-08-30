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
fn cancellation_removes_the_earliest_deadline() {
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

#[test]
fn cancellation_releases_storage_behind_a_live_earlier_deadline() {
    let now = Instant::now();
    let early = ParkToken::new(1, 1);
    let mut timers = TimerQueue::new();
    assert!(timers.schedule(early, now + Duration::from_secs(1)));

    for generation in 1..=256 {
        let later = ParkToken::new(2, generation);
        assert!(timers.schedule(later, now + Duration::from_secs(60)));
        assert!(timers.cancel(later));
    }

    assert_eq!(timers.next_deadline(), Some(now + Duration::from_secs(1)));
    assert_eq!(timers.active_count(), 1);
    assert_eq!(
        timers.deadlines.len(),
        1,
        "cancelled entries must not accumulate"
    );
    assert_eq!(
        timers.pop_expired(now + Duration::from_secs(60)),
        vec![early]
    );
}

#[test]
fn duplicate_registration_preserves_the_original_deadline() {
    let now = Instant::now();
    let token = ParkToken::new(1, 1);
    let mut timers = TimerQueue::new();
    assert!(timers.schedule(token, now + Duration::from_secs(1)));
    assert!(!timers.schedule(token, now + Duration::from_secs(2)));

    assert_eq!(
        timers.pop_expired(now + Duration::from_secs(1)),
        vec![token]
    );
    assert!(timers.pop_expired(now + Duration::from_secs(2)).is_empty());
    assert_eq!(timers.active_count(), 0);
}

#[test]
fn cancellation_is_exact_for_tokens_sharing_a_deadline() {
    let now = Instant::now();
    let old = ParkToken::new(1, 1);
    let current = ParkToken::new(1, 2);
    let other = ParkToken::new(2, 1);
    let mut timers = TimerQueue::new();
    assert!(timers.schedule(old, now));
    assert!(timers.schedule(current, now));
    assert!(timers.schedule(other, now));
    assert!(timers.cancel(old));
    assert!(!timers.cancel(old));

    assert_eq!(timers.pop_expired(now), vec![current, other]);
    assert_eq!(timers.active_count(), 0);
    assert_eq!(timers.next_deadline(), None);
}
